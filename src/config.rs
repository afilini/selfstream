use tokio::fs::File;
use tokio::io::AsyncReadExt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub listen: String,

    pub storage_dir: String,
    pub base_url: String,
    pub cdn_url: String,

    pub redis_server: String,

    pub stat_url: String,

    pub btcpay: BTCPayConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BTCPayConfig {
    pub key: String,
    pub url: String,
    pub merchant: String,
    pub webhook: String,
}

impl Config {
    pub async fn new() -> Result<Self, ConfigError> {
        let mut contents = vec![];

        let mut config_file = File::open("config.toml").await?;
        config_file.read_to_end(&mut contents).await?;

        Ok(toml::from_slice(&contents)?)
    }
}

#[derive(Debug)]
pub enum ConfigError {
    TokioIO(tokio::io::Error),
    TOML(toml::de::Error),
}

impl From<tokio::io::Error> for ConfigError {
    fn from(other: tokio::io::Error) -> Self {
        ConfigError::TokioIO(other)
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(other: toml::de::Error) -> Self {
        ConfigError::TOML(other)
    }
}
