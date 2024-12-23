use anyhow::Context;
use ozb::ozbargain::OZB_RSS_DEALS_URL;
use ozb::{skip_option, skip_result};
use reqwest::{
    header::{ETAG, IF_NONE_MATCH},
    StatusCode,
};
use sqlx::postgres::PgPoolOptions;
use std::collections::HashMap;
use std::time::Duration;
use tracing::Level;
use tracing_subscriber::{filter::Targets, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::registry()
        .with(Targets::default().with_default(Level::INFO))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = std::env::var("DATABASE_URL")?;

    tracing::info!("connecting to db: {database_url}");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let http_client = reqwest::ClientBuilder::new().build()?;

    let mut stored_etag: Option<String> = None;
    loop {
        tracing::info!("fetching rss");
        let response = http_client
            .get(OZB_RSS_DEALS_URL)
            .header(IF_NONE_MATCH, stored_etag.clone().unwrap_or_default())
            .send()
            .await?;

        if response.status() == StatusCode::NOT_MODIFIED {
            tracing::info!("304 response, skipping...");
            tracing::info!("sleeping :)");
            tokio::time::sleep(Duration::from_secs(60)).await;
            continue;
        } else {
            let resp_headers = response.headers();
            let etag = resp_headers
                .get(ETAG)
                .map(|h| h.to_str().unwrap_or_default().to_owned());

            stored_etag = etag;
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
                chrono::DateTime::parse_from_str(
                    skip_option!(item.pub_date(), "publication date"),
                    "%a, %d %b %Y %T %z",
                ),
                "publication date"
            )
            .naive_utc();
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

            let already_exists = sqlx::query!(
                "SELECT ozbargain_id FROM ozbargain_posts WHERE ozbargain_id = $1",
                &guid
            )
            .fetch_optional(&pool)
            .await?
            .is_some();

            if already_exists {
                tracing::warn!("this post already exists: {}", guid);
                continue;
            }

            let mut transaction = pool.begin().await?;

            let post = sqlx::query!(
                r#"INSERT INTO ozbargain_posts
                    (title,
                     description,
                     thumbnail,
                     link,
                     ozbargain_id,
                     publication_date,
                     notified)
                    VALUES ($1, $2, $3, $4, $5, $6, false)
                    ON CONFLICT (ozbargain_id) DO NOTHING
                    RETURNING id"#,
                title,
                description,
                thumbnail().ok(),
                link,
                &guid,
                publication_date
            )
            .fetch_one(&mut *transaction)
            .await?;

            let category_id_map = sqlx::query!(
                "SELECT id, name FROM categories WHERE name = ANY($1)",
                &categories
            )
            .fetch_all(&mut *transaction)
            .await?
            .into_iter()
            .map(|r| (r.name, r.id))
            .collect::<HashMap<String, i32>>();

            // FIXME:
            for category in categories {
                if let Some(category_id) = category_id_map.get(&category) {
                    sqlx::query!(
                        "INSERT INTO category_association (category_id, post_id) VALUES ($1, $2)",
                        category_id,
                        post.id
                    )
                    .execute(&mut *transaction)
                    .await?;
                }
            }

            transaction.commit().await?;
            tracing::info!("inserted: {}", guid);
        }

        tracing::info!("sleeping :)");
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
