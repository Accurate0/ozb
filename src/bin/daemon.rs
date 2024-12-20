use ::http::header::{ETAG, IF_NONE_MATCH};
use ::http::StatusCode;
use anyhow::Context;
use chrono::DateTime;
use lambda_http::{service_fn, Error};
use lambda_runtime::LambdaEvent;
use ozb::config::get_application_config;
use ozb::constants::cfg::{OZB_RSS_DEALS_URL, REDIS_KEY_PREFIX};
use ozb::http;
use ozb::prisma::{self, posts, trigger_ids};
use ozb::tracing::init_logger;
use ozb::{skip_option, skip_result};
use redis::AsyncCommands;
use serde_json::Value;
use tracing::{Instrument, Level};

#[tokio::main]
async fn main() -> Result<(), Error> {
    init_logger();
    let config = get_application_config().await?;
    let prisma_client = &prisma::new_client_with_url(&config.mongodb_connection_string).await?;
    let http_client = &http::get_default_http_client();
    let client = redis::Client::open(config.redis_connection_string.clone())?;
    let redis = &client.get_connection_manager().await.ok();
    let key = &format!("{}_{}", REDIS_KEY_PREFIX, "ETAG");

    lambda_runtime::run(service_fn(move |_: LambdaEvent<Value>| {
        async move {
            let redis = redis.clone();
            let etag: Option<String> = match redis.clone() {
                Some(mut redis) => match redis.get(key).await {
                    Ok(value) => value,
                    Err(e) => {
                        tracing::error!("error getting redis key: {}", e);
                        None
                    }
                },
                None => None,
            };

            let response = http_client
                .get(OZB_RSS_DEALS_URL)
                .header(IF_NONE_MATCH, etag.unwrap_or_default())
                .send()
                .await?;

            if response.status() == StatusCode::NOT_MODIFIED {
                tracing::info!("304 response, skipping...");
                return Ok(());
            } else {
                let resp_headers = response.headers();
                // update etag when content changed
                // set etag
                let etag = resp_headers
                    .get(ETAG)
                    .map(|h| h.to_str().unwrap_or_default());

                if let Some(etag) = etag {
                    if let Some(mut redis) = redis {
                        match redis.set::<_, _, ()>(key, etag).await {
                            Ok(_) => {}
                            Err(e) => tracing::error!("error setting redis key: {}", e),
                        };
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

                let categories: Vec<_> = item
                    .categories()
                    .iter()
                    .map(|c| c.name.to_owned().replace("&amp;", "&"))
                    .collect();

                tracing::info!("inserting: {} - {} - {}", guid, title, link);
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
                            vec![
                                posts::thumbnail::set(thumbnail().ok()),
                                posts::categories::set(categories.clone()),
                            ],
                        ),
                        vec![],
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
        }
        .instrument(tracing::span!(parent: None, Level::INFO, "ozb::check"))
    }))
    .await?;

    Ok(())
}
