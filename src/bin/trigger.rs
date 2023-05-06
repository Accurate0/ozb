use lambda_http::{service_fn, Error};
use lambda_runtime::LambdaEvent;
use ozb::{
    config::get_application_config,
    prisma::{self, posts::UniqueWhereParam},
    types::{Categories, MongoDbPayload},
};
use tl::{Bytes, ParserOptions};
use twilight_http::Client as DiscordHttpClient;
use twilight_model::{channel::message::AllowedMentions, id::Id};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, ImageSource};
use zephyrus::twilight_exports::ChannelMarker;

#[tokio::main]
async fn main() -> Result<(), Error> {
    foundation::log::init_logger(log::LevelFilter::Info, &[]);
    let config = get_application_config().await?;
    let discord_http = &DiscordHttpClient::new(config.discord_token.to_owned());
    let prisma_client = &prisma::new_client_with_url(&config.mongodb_connection_string).await?;

    lambda_runtime::run(service_fn(
        move |event: LambdaEvent<MongoDbPayload>| async move {
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

            log::info!("[new deal] id: {}", event.payload.id);
            log::info!("title: {}, {}", title, link);
            log::info!("description: {}", description.replace('\n', ""));

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
                    || post_categories
                        .iter()
                        .any(|p| keyword_categories.iter().any(|c| p == c));

                let trigger_condition = title_or_description && category_matches;

                log::info!("keyword: {}", keyword);
                log::info!("keyword categories: {:?}", keyword_categories);
                log::info!("post categories: {:?}", post_categories);
                log::info!("title/description match: {}", title_or_description);
                log::info!("category match: {}", category_matches);

                if trigger_condition {
                    log::info!("triggered for {} [{}]", keyword, data.user_id);
                    let embed = EmbedBuilder::default()
                        .color(0xde935f)
                        .title("OzBargain")
                        .field(EmbedFieldBuilder::new("Title", title.clone()))
                        .field(EmbedFieldBuilder::new("Link", link.clone()))
                        .field(EmbedFieldBuilder::new("Keyword", keyword.clone()))
                        .field(EmbedFieldBuilder::new(
                            "Categories",
                            post_categories
                                .iter()
                                .map(|p| p.to_string())
                                .collect::<Vec<String>>()
                                .join(", "),
                        ));
                    let embed = if let Some(ref thumbnail) = thumbnail {
                        embed.thumbnail(ImageSource::url(thumbnail.clone())?)
                    } else {
                        embed
                    };

                    let allowed_mentions = AllowedMentions {
                        parse: vec![],
                        users: Vec::from([Id::new(data.user_id.parse()?)]),
                        roles: vec![],
                        replied_user: false,
                    };

                    discord_http
                        .create_message(Id::<ChannelMarker>::new(data.channel_id.parse()?))
                        .embeds(&[embed.build()])?
                        .allowed_mentions(Some(&allowed_mentions))
                        .content(&format!("<@{}>", data.user_id))?
                        .await?;

                    prisma_client
                        .audit_entries()
                        .create(
                            serde_json::to_value(full_document.clone())?,
                            serde_json::to_value(data)?,
                            vec![],
                        )
                        .exec()
                        .await?;
                }
            }

            Ok::<(), Error>(())
        },
    ))
    .await?;
    Ok(())
}
