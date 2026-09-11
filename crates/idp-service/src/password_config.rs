use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct PasswordConfig {
    pub salt_length: usize,
    pub hash_length: u32,
    pub memory_mib: u32,
    pub iterations: u32,
    pub parallelism: u32,
    pub history: u8,
    pub expire_days: u8,
    pub force_reset_after_days: Option<u8>,
}

impl Default for PasswordConfig {
    fn default() -> Self {
        Self {
            salt_length: 16,
            hash_length: 32,
            memory_mib: 47,
            iterations: 3,
            parallelism: 4,
            history: 24,
            expire_days: 60,
            force_reset_after_days: Some(90),
        }
    }
}
