use std::path::Path;

use api::{Environment, ServerConfig};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub log_level: String,
    pub api_public_uri: String,
    pub data_dir: String,
    pub idp_api_base_uri: String,
    pub idp_issuer_uri: String,
    pub idp_oauth_client_id: Option<String>,
    pub idp_oauth_client_secret: Option<String>,
    pub idp_service_audience: Option<String>,
    pub management_api_base_uri: String,
    pub management_oauth_client_id: Option<String>,
    pub management_oauth_client_secret: Option<String>,
    pub management_service_audience: Option<String>,
    pub env: Environment,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            api_public_uri: "https://storage-api.localhost:1355".to_string(),
            data_dir: "storage-data".to_string(),
            idp_api_base_uri: String::new(),
            idp_issuer_uri: String::new(),
            idp_oauth_client_id: None,
            idp_oauth_client_secret: None,
            idp_service_audience: None,
            management_api_base_uri: String::new(),
            management_oauth_client_id: None,
            management_oauth_client_secret: None,
            management_service_audience: None,
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
            .add_source(config::Environment::with_prefix("STORAGE"))
            .build()?
            .try_deserialize()
    }
}
