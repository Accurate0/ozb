use chrono_tz::Australia;
use config::{Config, Environment};
use foundation::{aws, config::sources::secret_manager::SecretsManagerSource};
use lambda_http::{service_fn, Error};
use lambda_runtime::LambdaEvent;
use ozb::{
    prisma,
    types::{ApplicationConfig, MongoDbPayload},
};
use twilight_http::Client as DiscordHttpClient;
use twilight_model::{channel::message::AllowedMentions, id::Id};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, ImageSource};
use zephyrus::twilight_exports::ChannelMarker;

#[tokio::main]
async fn main() -> Result<(), Error> {
    foundation::log::init_logger(
        log::LevelFilter::Info,
        &[
            "twilight_http_ratelimiting::in_memory::bucket",
            "twilight_gateway::shard",
        ],
    );
    lambda_runtime::run(service_fn(run)).await?;
    Ok(())
}

async fn run(event: LambdaEvent<MongoDbPayload>) -> Result<(), anyhow::Error> {
    let shared_config = aws::config::get_shared_config().await;
    let secrets = aws_sdk_secretsmanager::Client::new(&shared_config);

    let secret_manager_source = SecretsManagerSource::new("Ozb-", secrets);
    let config = Config::builder()
        .add_async_source(secret_manager_source)
        .add_source(Environment::default().prefix("OZB"))
        .build()
        .await?
        .try_deserialize::<ApplicationConfig>()?;

    let discord_http = DiscordHttpClient::new(config.discord_token.to_owned());
    let prisma_client = prisma::new_client_with_url(&config.mongodb_connection_string).await?;

    let active_keywords = prisma_client
        .registered_keywords()
        .find_many(vec![])
        .exec()
        .await?;

    let title = event.payload.detail.full_document.title;
    let description = event.payload.detail.full_document.description;
    let link = event.payload.detail.full_document.link;
    let thumbnail = event.payload.detail.full_document.thumbnail;

    for data in active_keywords {
        let keyword = data.keyword;
        if title.contains(&keyword) || description.contains(&keyword) {
            let embed = EmbedBuilder::default()
                .title("OzBargain")
                .field(EmbedFieldBuilder::new("Link", link.clone()))
                .field(EmbedFieldBuilder::new("Keyword", keyword.clone()))
                .field(EmbedFieldBuilder::new(
                    "Set at",
                    data.added_at.with_timezone(&Australia::Perth).to_rfc2822(),
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
        }
    }

    Ok(())
}
