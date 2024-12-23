use mongodb::{
    bson::doc,
    options::{ClientOptions, ServerApi, ServerApiVersion},
    Client,
};
use sqlx::postgres::PgPoolOptions;
use tracing::Level;
use tracing_subscriber::{filter::Targets, layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Debug, serde::Deserialize)]
struct RegisteredKeywords {
    keyword: String,
    user_id: String,
    channel_id: String,
    categories: Option<Vec<String>>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::registry()
        .with(Targets::default().with_default(Level::INFO))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = std::env::var("DATABASE_URL")?;
    let mongodb_connection_string = std::env::var("MONGODB_CONNECTION_STRING")?;

    let mut client_options = ClientOptions::parse(mongodb_connection_string).await?;
    let server_api = ServerApi::builder().version(ServerApiVersion::V1).build();
    client_options.server_api = Some(server_api);
    let client = Client::with_options(client_options)?;

    tracing::info!("connecting to db: {database_url}");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await?;

    let keywords = client
        .database("ozb-db")
        .collection::<RegisteredKeywords>("RegisteredKeywords");
    let mut all_keywords = keywords.find(doc! {}).await?;

    let mut transaction = pool.begin().await?;

    while all_keywords.advance().await? {
        let registered_keyword = all_keywords.deserialize_current()?;

        tracing::info!("migrating: {registered_keyword:?}");

        let existing_record = sqlx::query!(
            r#"
                SELECT rk.* FROM registered_keywords as rk
                JOIN discord_users AS du ON rk.discord_user_id = du.id
                WHERE keyword = $1 AND du.discord_id = $2
            "#,
            registered_keyword.keyword,
            registered_keyword.user_id
        )
        .fetch_optional(&mut *transaction)
        .await?;

        if existing_record.is_some() {
            tracing::info!("already migrated, skipped");
            continue;
        }

        sqlx::query!(
            "INSERT INTO discord_users(discord_id) VALUES ($1) ON CONFLICT DO NOTHING",
            registered_keyword.user_id
        )
        .execute(&mut *transaction)
        .await?;

        let discord_user_id = sqlx::query!(
            "SELECT id FROM discord_users WHERE discord_id = $1",
            registered_keyword.user_id
        )
        .fetch_one(&mut *transaction)
        .await?;

        let discord_notification_id = sqlx::query!(
            "INSERT INTO discord_notifications (channel_id) VALUES ($1) RETURNING id",
            registered_keyword.channel_id
        )
        .fetch_one(&mut *transaction)
        .await?;

        let categories = registered_keyword.categories.unwrap_or_default();
        sqlx::query!(
            "INSERT INTO registered_keywords (keyword, discord_user_id, discord_notification_id, categories) VALUES ($1, $2, $3, $4)",
            registered_keyword.keyword,
            discord_user_id.id,
            discord_notification_id.id,
            &categories
        )
        .execute(&mut *transaction)
        .await?;
    }

    transaction.commit().await?;

    Ok(())
}
