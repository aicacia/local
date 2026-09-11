#[cfg(not(feature = "std"))]
use alloc::{
    string::{String, ToString},
    vec,
    vec::Vec,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct BootstrapConfig {
    pub is_master: bool,
    pub web: bool,
    pub desktop: bool,
    pub device_name: String,
    pub idp_url: String,
    pub management_url: String,
    pub admin_username: String,
    pub admin_email: String,
    pub admin_password: String,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            is_master: true,
            web: false,
            desktop: false,
            device_name: "API Server".to_string(),
            idp_url: "https://lidp.localhost:1337".to_string(),
            management_url: "https://idp-management.localhost:1337".to_string(),
            admin_username: "admin".to_string(),
            admin_email: "admin@localhost".to_string(),
            admin_password: "password".to_string(),
        }
    }
}
