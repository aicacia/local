use std::path::Path;

use api::{Environment, ServerConfig};
use bootstrap_service::bootstrap::BootstrapConfig;
use db::DatabaseConfig;
use idp_service::{PasswordConfig, oauth2::OAuth2Config};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub oauth2: OAuth2Config,
    pub bootstrap: BootstrapConfig,
    pub password: PasswordConfig,
    pub key_namespace: String,
    pub log_level: String,
    pub ui_public_uri: String,
    pub api_public_uri: String,
    pub env: Environment,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            database: DatabaseConfig::default(),
            oauth2: OAuth2Config::default(),
            bootstrap: BootstrapConfig::default(),
            password: PasswordConfig::default(),
            key_namespace: "idp-management".to_string(),
            log_level: "DEBUG".to_string(),
            ui_public_uri: "https://idp-management.localhost:1337".to_string(),
            api_public_uri: "https://management-api.localhost:1337".to_string(),
            env: Environment::default(),
        }
    }
}

impl<'a> TryFrom<&'a Path> for AppConfig {
    type Error = config::ConfigError;

    fn try_from(config_path: &'a Path) -> Result<Self, Self::Error> {
        config::Config::builder()
            .add_source(config::File::with_name(
                config_path.to_string_lossy().as_ref(),
            ))
            .add_source(config::Environment::with_prefix("SERVER"))
            .build()?
            .try_deserialize()
    }
}
