use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use std::{sync::Arc, time::Duration};
use tracing::Level;
use tracing_subscriber::{filter::Targets, layer::SubscriberExt, util::SubscriberInitExt};
use twilight_http::Client as DiscordHttpClient;
use twilight_model::{
    channel::message::AllowedMentions,
    id::{marker::ChannelMarker, Id},
};
use twilight_util::builder::embed::{EmbedBuilder, EmbedFieldBuilder, ImageSource};

type State = state::TypeMap![Send + Sync];
struct MatchedDetails {
    title: String,
    post_id: i32,
    link: String,
    channel_id: String,
    discord_id: String,
    categories: Vec<String>,
    thumbnail: Option<String>,
}

async fn process_message(state: Arc<State>) -> Result<(), anyhow::Error> {
    tracing::info!("processing message");
    let pool = state.get::<Pool<Postgres>>();
    let mut transaction = pool.begin().await?;
    let discord_http_client = state.get::<DiscordHttpClient>();

    let posts = sqlx::query!(
        r#"SELECT * from ozbargain_posts WHERE notified = false LIMIT 10 FOR UPDATE SKIP LOCKED"#,
    )
    .fetch_all(&mut *transaction)
    .await?;

    tracing::info!("processing: {}", posts.len());

    let ids = posts.iter().map(|p| p.id).collect::<Vec<_>>();
    // notified means processed..
    sqlx::query!(
        "UPDATE ozbargain_posts SET notified = true WHERE id = ANY($1)",
        &ids
    )
    .execute(&mut *transaction)
    .await?;

    let current_keywords = sqlx::query!(
        r#"
                SELECT rk.*, du.discord_id, dn.channel_id FROM registered_keywords as rk
                JOIN discord_users AS du on rk.discord_user_id = du.id
                JOIN discord_notifications AS dn on rk.discord_notification_id = dn.id
                "#
    )
    .fetch_all(&mut *transaction)
    .await?;

    let mut matched_keywords = vec![];
    for ozbargain_post in posts {
        for keyword_data in &current_keywords {
            let keyword = keyword_data.keyword.to_ascii_lowercase();
            let keyword_categories = &keyword_data.categories;

            // dbg!(&keyword);
            // dbg!(&keyword_data);
            // dbg!(&keyword_categories);

            let does_match = {
                let title = &ozbargain_post.title;
                let description = &ozbargain_post.description;

                let post_categories = sqlx::query!(
                    r#"SELECT name FROM category_association as ca
                            JOIN categories as c on ca.category_id = c.id WHERE post_id = $1"#,
                    ozbargain_post.id
                )
                .fetch_all(pool)
                .await?
                .iter()
                .map(|pc| pc.name.clone())
                .collect::<Vec<_>>();

                let description = tl::parse(description, tl::ParserOptions::default())
                    .map(|dom| {
                        let mut string_list = Vec::new();
                        for node in dom.nodes() {
                            if let Some(tag) = node.as_tag() {
                                if tag.name() == "img" {
                                    string_list.push(
                                        tag.attributes()
                                            .get("alt")
                                            .unwrap_or(None)
                                            .unwrap_or(&tl::Bytes::from(""))
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

                let title_or_description = title.to_ascii_lowercase().contains(&keyword)
                    || description.to_ascii_lowercase().contains(&keyword);

                let category_matches = keyword_categories.is_empty()
                    || keyword_categories.iter().any(|c| *c == "All")
                    || keyword_categories
                        .iter()
                        .any(|p| post_categories.iter().any(|c| p == c));

                (
                    title_or_description && category_matches,
                    MatchedDetails {
                        title: ozbargain_post.title.clone(),
                        link: ozbargain_post.link.clone(),
                        post_id: ozbargain_post.id,
                        categories: post_categories,
                        thumbnail: ozbargain_post.thumbnail.clone(),
                        channel_id: keyword_data.channel_id.clone(),
                        discord_id: keyword_data.discord_id.clone(),
                    },
                )
            };

            if does_match.0 {
                matched_keywords.push((keyword_data, does_match.1));
            }
        }
    }
    tracing::info!("matched {}", matched_keywords.len());
    for matched in matched_keywords {
        let title = matched.1.title;
        let link = matched.1.link;
        let keyword = &matched.0.keyword;
        let post_categories = matched.1.categories;
        let thumbnail = matched.1.thumbnail;
        // FIXME: type should be prefix
        let user_id = matched.1.discord_id;
        // FIXME: could be webhook
        let channel_id = matched.1.channel_id;

        let embed = EmbedBuilder::default()
            .color(0xde935f)
            .title("OzBargain")
            .field(EmbedFieldBuilder::new("Title", title))
            .field(EmbedFieldBuilder::new("Link", link))
            .field(EmbedFieldBuilder::new("Keyword", keyword))
            .field(EmbedFieldBuilder::new(
                "Categories",
                post_categories.join(", "),
            ));

        let embed = if let Some(ref thumbnail) = thumbnail {
            embed.thumbnail(ImageSource::url(thumbnail)?)
        } else {
            embed
        };

        let allowed_mentions = AllowedMentions {
            parse: vec![],
            users: Vec::from([Id::new(user_id.parse()?)]),
            roles: vec![],
            replied_user: false,
        };

        if let Err(e) = discord_http_client
            .create_message(Id::<ChannelMarker>::new(channel_id.parse()?))
            .embeds(&[embed.build()])?
            .allowed_mentions(Some(&allowed_mentions))
            .content(&format!("<@{}>", user_id))?
            .await
        {
            tracing::error!("error sending notif: {e}");
        };

        tracing::info!("discord: notification sent {} {}", channel_id, user_id);

        sqlx::query!(
            r#"INSERT INTO audit
                            (post_id, keyword_id, user_id)
                            VALUES
                            ($1, $2, $3)"#,
            matched.1.post_id,
            matched.0.id,
            matched.0.discord_user_id
        )
        .execute(&mut *transaction)
        .await?;
    }

    transaction.commit().await?;

    Ok::<(), anyhow::Error>(())
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::registry()
        .with(Targets::default().with_default(Level::INFO))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = std::env::var("DATABASE_URL")?;
    let discord_token = std::env::var("DISCORD_TOKEN")?;

    let discord_http_client = DiscordHttpClient::new(discord_token);

    tracing::info!("connecting to db: {database_url}");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let state = Arc::new(State::default());
    state.set(pool);
    state.set(discord_http_client);

    loop {
        if let Err(e) = process_message(state.clone()).await {
            tracing::error!("{e}")
        }

        tracing::info!("sleeping");
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
