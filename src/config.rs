use crate::{source::SecretsManagerSource, types::ApplicationConfig};
use aws_config::{
    environment::EnvironmentVariableRegionProvider, retry::RetryConfig, BehaviorVersion,
};
use config::{Config, Environment};

pub async fn get_application_config() -> Result<ApplicationConfig, anyhow::Error> {
    let shared_config = aws_config::defaults(BehaviorVersion::v2024_03_28())
        .credentials_provider(EnvironmentVariableRegionProvider::new())
        .region("ap-southeast-2")
        .retry_config(RetryConfig::standard())
        .load()
        .await;

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
