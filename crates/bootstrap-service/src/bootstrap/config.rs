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
    pub web: bool,
    pub desktop: bool,
    pub idp_url: String,
    pub management_url: String,
}

impl Default for BootstrapConfig {
    fn default() -> Self {
        Self {
            web: false,
            desktop: false,
            idp_url: "https://lidp.localhost:1355".to_string(),
            management_url: "https://idp-management.localhost:1355".to_string(),
        }
    }
}
