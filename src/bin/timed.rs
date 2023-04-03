use chrono::Days;
use config::{Config, Environment};
use foundation::{aws, config::sources::secret_manager::SecretsManagerSource};
use lambda_http::{service_fn, Error};
use lambda_runtime::LambdaEvent;
use ozb::{
    prisma::{self, posts, read_filters::DateTimeFilter},
    types::ApplicationConfig,
};
use serde_json::Value;

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

    lambda_runtime::run(service_fn(move |_: LambdaEvent<Value>| async move {
        let datetime_2_weeks_ago = chrono::Utc::now()
            .checked_sub_days(Days::new(14))
            .ok_or(anyhow::Error::msg("Day not in range?"))?;

        let deleted = prisma_client
            .posts()
            .find_many(vec![posts::WhereParam::AddedAt(DateTimeFilter::Lt(
                datetime_2_weeks_ago.into(),
            ))])
            .exec()
            .await?;

        log::info!("deleted {:#?} entries", deleted);

        Ok::<(), Error>(())
    }))
    .await?;
    Ok(())
}
