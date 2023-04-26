use crate::types::ApplicationConfig;
use config::{Config, Environment};
use foundation::{aws, config::sources::secret_manager::SecretsManagerSource};

pub async fn get_application_config() -> Result<ApplicationConfig, anyhow::Error> {
    let shared_config = aws::config::get_shared_config().await;

    let secrets = aws_sdk_secretsmanager::Client::new(&shared_config);
    let secret_manager_source = SecretsManagerSource::new("Ozb-", secrets.clone());
    let shared_secrets_source = SecretsManagerSource::new("Shared-", secrets).with_required(false);

    Ok(Config::builder()
        .add_async_source(secret_manager_source)
        .add_async_source(shared_secrets_source)
        .add_source(Environment::default().prefix("OZB"))
        .build()
        .await?
        .try_deserialize::<ApplicationConfig>()?)
}
