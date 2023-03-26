use crate::services::{bot::run_discord_bot, daemon::run_ozbd};
use config::{Config, Environment};
use foundation::{aws, config::sources::secret_manager::SecretsManagerSource};
use ozb::types::ApplicationConfig;

mod services;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let is_daemon = std::env::args().any(|a| a == "--daemon");
    foundation::log::init_logger(
        log::LevelFilter::Info,
        &[
            "twilight_http_ratelimiting::in_memory::bucket",
            "twilight_gateway::shard",
        ],
    );
    log::info!("daemon process: {}", is_daemon);

    let shared_config = aws::config::get_shared_config().await;
    let secrets = aws_sdk_secretsmanager::Client::new(&shared_config);

    let secret_manager_source = SecretsManagerSource::new("Ozb-", secrets);
    let config = Config::builder()
        .add_async_source(secret_manager_source)
        .add_source(Environment::default().prefix("OZB"))
        .build()
        .await?
        .try_deserialize::<ApplicationConfig>()?;

    if is_daemon {
        run_ozbd(config).await?
    } else {
        run_discord_bot(config).await?
    }

    Ok(())
}
