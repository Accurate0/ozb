use anyhow::Context;
use chrono::DateTime;
use http::StatusCode;
use ozb::{
    constants::cfg::{OZB_RSS_DEALS_URL, SLEEP_DURATION_SECS},
    prisma::{
        self,
        posts::{thumbnail, UniqueWhereParam},
    },
    types::ApplicationConfig,
};
use reqwest::ClientBuilder;
use rss::Channel;
use std::{thread, time::Duration};

pub async fn run_ozbd(config: ApplicationConfig) -> Result<(), anyhow::Error> {
    let client = prisma::new_client_with_url(&config.mongodb_connection_string).await?;
    let http_client =
        foundation::http::get_default_middleware(ClientBuilder::new().build()?).build();
    let mut etag = None;
    let sleep = || {
        log::info!("sleeping...");
        thread::sleep(Duration::from_secs(SLEEP_DURATION_SECS))
    };

    loop {
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
            sleep();
            continue;
        } else {
            let resp_headers = response.headers().clone();
            // update etag when content changed
            etag = resp_headers
                .get("etag")
                .map(|h| h.to_str().unwrap().to_owned());
        }

        let response = response.bytes().await?;
        let channel = Channel::read_from(&response[..])?;

        for item in channel.items() {
            let guid = item
                .guid()
                .unwrap()
                .value()
                .split_ascii_whitespace()
                .next()
                .unwrap()
                .to_owned();
            let title = item.title().unwrap().to_owned();
            // Sun, 26 Mar 2023 17:29:29 +1100
            let publication_date =
                DateTime::parse_from_str(item.pub_date().unwrap(), "%a, %d %b %Y %T %z")?;
            let link = item.link().unwrap().to_owned();
            let description = item.description().unwrap().to_owned();
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

            client
                .posts()
                .upsert(
                    UniqueWhereParam::OzbIdEquals(guid.clone()),
                    (
                        guid,
                        title,
                        publication_date,
                        link,
                        description,
                        vec![thumbnail::set(thumbnail().ok())],
                    ),
                    vec![thumbnail::set(thumbnail().ok())],
                )
                .exec()
                .await?;
        }

        sleep();
    }
}
