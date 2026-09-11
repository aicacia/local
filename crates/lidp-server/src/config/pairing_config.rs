use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PairingConfig {
    pub accepting_timeout_seconds: u64,
}

impl Default for PairingConfig {
    fn default() -> Self {
        Self {
            accepting_timeout_seconds: 5 * 60,
        }
    }
}
