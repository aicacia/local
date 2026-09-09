use std::path::Path;

use api::{Environment, ServerConfig};
use db::DatabaseConfig;
use lidp_service::{PasswordConfig, bootstrap::BootstrapConfig, oauth2::OAuth2Config};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub device_key: String,
    pub oauth2: OAuth2Config,
    pub bootstrap: BootstrapConfig,
    pub password: PasswordConfig,
    pub key_namespace: String,
    pub control_plane_uri: Option<String>,
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
            device_key: "device.key".to_string(),
            oauth2: OAuth2Config::default(),
            bootstrap: BootstrapConfig::default(),
            password: PasswordConfig::default(),
            key_namespace: "lidp".to_string(),
            control_plane_uri: None,
            ui_public_uri: "https://lidp.localhost:1337".to_string(),
            api_public_uri: "https://lidp-api.localhost:1337".to_string(),
            log_level: "DEBUG".to_string(),
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
            .add_source(config::Environment::with_prefix("LIDP"))
            .build()?
            .try_deserialize()
    }
}
