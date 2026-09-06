#[cfg(not(feature = "std"))]
use alloc::string::String;

use chrono::{DateTime, Utc};

use crate::contract::UserDeviceState;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UserDevice {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub public_key: String,
    pub address: String,
    #[serde(with = "super::sql_enum::user_device_state")]
    pub state: UserDeviceState,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub updated_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub revoked_at: Option<DateTime<Utc>>,
}
