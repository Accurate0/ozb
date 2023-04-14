use crate::services::bot::run_discord_bot;
use ozb::config::get_application_config;

mod services;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    foundation::log::init_logger(
        log::LevelFilter::Info,
        &[
            "twilight_http_ratelimiting::in_memory::bucket",
            "twilight_gateway::shard",
        ],
    );

    let config = get_application_config().await?;
    run_discord_bot(config).await?;

    Ok(())
}
