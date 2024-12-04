use itertools::Itertools;
use lambda_http::{service_fn, Error};
use lambda_runtime::LambdaEvent;
use ozb::{
    config::get_application_config,
    prisma::{self, posts::UniqueWhereParam},
    tracing::init_logger,
    types::{Categories, MongoDbPayload},
};
use std::{collections::HashMap, future::IntoFuture};
use tl::{Bytes, ParserOptions};
use tracing::{Instrument, Level};
use twilight_http::Client as DiscordHttpClient;
use twilight_model::{channel::message::AllowedMentions, id::Id};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, ImageSource};
use vesper::twilight_exports::ChannelMarker;

#[derive(Clone)]
struct Trigger {
    keyword_data: prisma::registered_keywords::Data,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    init_logger();
    let config = get_application_config().await?;
    let discord_http = &DiscordHttpClient::new(config.discord_token.to_owned());
    let prisma_client = &prisma::new_client_with_url(&config.mongodb_connection_string).await?;

    lambda_runtime::run(service_fn(move |event: LambdaEvent<MongoDbPayload>| {
        async move {
            let document_id = event.payload.detail.full_document.post_id;
            let full_document = prisma_client
                .posts()
                .find_unique(UniqueWhereParam::IdEquals(document_id.clone()))
                .exec()
                .await?
                .ok_or(anyhow::Error::msg(format!(
                    "document {}: not found",
                    document_id
                )))?;

            let active_keywords = prisma_client
                .registered_keywords()
                .find_many(vec![])
                .exec()
                .await?;

            let title = &full_document.title;
            let description = &full_document.description;

            let description = tl::parse(description, ParserOptions::default())
                .map(|dom| {
                    let mut string_list = Vec::new();
                    for node in dom.nodes() {
                        if let Some(tag) = node.as_tag() {
                            if tag.name() == "img" {
                                string_list.push(
                                    tag.attributes()
                                        .get("alt")
                                        .unwrap_or(None)
                                        .unwrap_or(&Bytes::from(""))
                                        .as_utf8_str()
                                        .to_string(),
                                )
                            }
                        };

                        string_list.push(node.inner_text(dom.parser()).to_string())
                    }

                    string_list.join("\n")
                })
                .unwrap_or(description.to_owned());

            let link = &full_document.link;
            let thumbnail = &full_document.thumbnail;
            let post_categories = &full_document.categories;

            tracing::info!({ title = title, link }, "[new deal] id: {}", event.payload.id);

            let mut channel_id_to_trigger = HashMap::<String, Vec<Trigger>>::new();

            // todo: add regex support
            for data in active_keywords {
                let keyword: String = data.keyword.to_ascii_lowercase();
                let keyword_categories = &data.categories;

                let title_or_description = title.to_ascii_lowercase().contains(&keyword)
                    || description.to_ascii_lowercase().contains(&keyword);

                let category_matches = keyword_categories.is_empty()
                    || keyword_categories
                        .iter()
                        .any(|c| *c == Categories::All.to_string())
                    || keyword_categories
                        .iter()
                        .any(|p| post_categories.iter().any(|c| p == c));

                let trigger_condition = title_or_description && category_matches;

                tracing::info!({
                        keyword_categories = ?keyword_categories,
                        post_categories = ?post_categories,
                        title_or_description_match = title_or_description,
                        category_match = category_matches
                    },
                    "keyword: {}",
                    keyword,
                );

                if trigger_condition {
                    tracing::info!("triggered for {} [{}]", keyword, data.user_id);
                    let channel_id = data.channel_id.clone();
                    let trigger_value = Trigger { keyword_data: data };
                    channel_id_to_trigger
                        .entry(channel_id)
                        .and_modify(|v| v.push(trigger_value.clone()))
                        .or_insert(vec![trigger_value]);
                }
            }

            for (channel_id, trigger) in channel_id_to_trigger {
                let pings = trigger
                    .iter()
                    .map(|t| t.keyword_data.user_id.clone())
                    .unique()
                    .collect::<Vec<_>>();
                let keywords = trigger
                    .iter()
                    .map(|t| t.keyword_data.keyword.clone())
                    .unique()
                    .collect::<Vec<_>>();

                let embed = EmbedBuilder::default()
                    .color(0xde935f)
                    .title("OzBargain")
                    .field(EmbedFieldBuilder::new("Title", title.clone()))
                    .field(EmbedFieldBuilder::new("Link", link.clone()))
                    .field(EmbedFieldBuilder::new("Keywords", keywords.join(", ")))
                    .field(EmbedFieldBuilder::new(
                        "Categories",
                        post_categories.join(", "),
                    ));
                let embed = if let Some(ref thumbnail) = thumbnail {
                    embed.thumbnail(ImageSource::url(thumbnail.clone())?)
                } else {
                    embed
                };

                let allowed_mentions = AllowedMentions {
                    parse: vec![],
                    users: pings
                        .iter()
                        .map(|id| Id::new(id.parse().unwrap()))
                        .collect(),
                    roles: vec![],
                    replied_user: false,
                };

                let maybe_err = discord_http
                    .create_message(Id::<ChannelMarker>::new(channel_id.parse()?))
                    .embeds(&[embed.build()])?
                    .allowed_mentions(Some(&allowed_mentions))
                    .content(
                        &pings
                            .iter()
                            .map(|id| format!("<@{}>", id))
                            .collect::<Vec<_>>()
                            .join(" "),
                    )?
                    .into_future()
                    .instrument(tracing::span!(Level::INFO, "ozb::discord::message"))
                    .await;

                if let Err(e) = maybe_err {
                    tracing::error!("error in discord: {e}");
                }

                let keyword_data = trigger
                    .into_iter()
                    .map(|t| t.keyword_data)
                    .collect::<Vec<_>>();

                prisma_client
                    .audit_entries()
                    .create(
                        serde_json::to_value(full_document.clone())?,
                        serde_json::to_value(keyword_data)?,
                        vec![],
                    )
                    .exec()
                    .await?;
            }

            Ok::<(), Error>(())
        }
        .instrument(tracing::span!(parent: None, Level::INFO, "ozb::new_deal"))
    }))
    .await?;
    Ok(())
}
