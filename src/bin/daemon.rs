use ::http::header::ETAG;
use ::http::StatusCode;
use anyhow::Context;
use chrono::DateTime;
use config::{Config, Environment};
use foundation::http;
use foundation::{aws, config::sources::secret_manager::SecretsManagerSource};
use lambda_http::{service_fn, Error};
use lambda_runtime::LambdaEvent;
use ozb::constants::cfg::{OZB_RSS_DEALS_URL, REDIS_KEY_PREFIX};
use ozb::{
    prisma::{self, posts, trigger_ids},
    types::ApplicationConfig,
};
use redis::AsyncCommands;
use reqwest::ClientBuilder;
use serde_json::Value;

macro_rules! skip_option {
    ($res:expr, $item:literal) => {
        match $res {
            Some(val) => val,
            None => {
                log::warn!("skipping loop because {} missing", $item);
                continue;
            }
        }
    };
}

macro_rules! skip_result {
    ($res:expr, $item:literal) => {
        match $res {
            Ok(val) => val,
            Err(e) => {
                log::warn!("skipping loop because {} missing, error: {}", $item, e);
                continue;
            }
        }
    };
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    foundation::log::init_logger(log::LevelFilter::Info, &[]);
    let shared_config = aws::config::get_shared_config().await;
    let secrets = aws_sdk_secretsmanager::Client::new(&shared_config);
    let secret_manager_source = SecretsManagerSource::new("Ozb-", secrets);
    let config = Config::builder()
        .add_async_source(secret_manager_source)
        .add_source(Environment::default().prefix("OZB"))
        .build()
        .await?
        .try_deserialize::<ApplicationConfig>()?;
    let prisma_client = &prisma::new_client_with_url(&config.mongodb_connection_string).await?;
    let http_client = &http::get_default_middleware(ClientBuilder::new().build()?).build();
    let client = redis::Client::open(config.redis_connection_string.clone())?;
    let redis = &client.get_tokio_connection_manager().await.ok();
    let key = &format!("{}_{}", REDIS_KEY_PREFIX, "ETAG");

    lambda_runtime::run(service_fn(move |_: LambdaEvent<Value>| async move {
        let redis = redis.clone();
        let etag = match redis.clone() {
            Some(mut redis) => match redis.get(key).await {
                Ok(value) => value,
                Err(_) => None,
            },
            None => None,
        };

        let response = http_client
            .get(OZB_RSS_DEALS_URL)
            .header(
                "If-None-Match",
                etag.clone().unwrap_or("".to_owned()).to_string(),
            )
            .send()
            .await?;

        if response.status() == StatusCode::NOT_MODIFIED {
            log::info!("304 response, skipping...");
            return Ok(());
        } else {
            let resp_headers = response.headers().clone();
            // update etag when content changed
            // set etag
            let etag = resp_headers
                .get(ETAG)
                .map(|h| h.to_str().unwrap().to_owned());

            if let Some(etag) = etag {
                if let Some(mut redis) = redis {
                    redis.set(key, etag).await?
                }
            };
        }

        let response = response.bytes().await?;
        let channel = rss::Channel::read_from(&response[..])?;

        for item in channel.items() {
            let guid = skip_option!(item.guid(), "guid")
                .value()
                .split_ascii_whitespace()
                .next();

            let guid = skip_option!(guid, "guid whitespace").to_owned();

            let title = skip_option!(item.title(), "title").to_owned();
            // Sun, 26 Mar 2023 17:29:29 +1100
            let publication_date = skip_result!(
                DateTime::parse_from_str(
                    skip_option!(item.pub_date(), "publication date"),
                    "%a, %d %b %Y %T %z",
                ),
                "publication date"
            );
            let link = skip_option!(item.link(), "link").to_owned();
            let description = skip_option!(item.description(), "description").to_owned();
            let ext = item.extensions();
            let thumbnail = || -> Result<String, anyhow::Error> {
                Ok(ext
                    .get("media")
                    .context("must have thumbnail")?
                    .get("thumbnail")
                    .context("must have url 1")?
                    .first()
                    .context("must have url 2")?
                    .attrs
                    .get("url")
                    .context("must have url 3")?
                    .to_owned())
            };

            log::info!("inserting: {} - {} - {}", guid, title, link);
            let added = prisma_client
                .posts()
                .upsert(
                    posts::UniqueWhereParam::OzbIdEquals(guid.clone()),
                    (
                        guid,
                        title,
                        publication_date,
                        link,
                        description,
                        vec![posts::thumbnail::set(thumbnail().ok())],
                    ),
                    vec![posts::thumbnail::set(thumbnail().ok())],
                )
                .exec()
                .await?;

            // this is the actual trigger
            prisma_client
                .trigger_ids()
                .upsert(
                    trigger_ids::UniqueWhereParam::PostIdEquals(added.id.clone()),
                    (added.id, vec![]),
                    vec![],
                )
                .exec()
                .await?;
        }

        Ok::<(), Error>(())
    }))
    .await?;

    Ok(())
}
