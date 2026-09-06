#[cfg(not(feature = "std"))]
use alloc::string::String;

use chrono::{DateTime, Utc};

use crate::contract::{DeviceInfo, UserDeviceState};

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

impl From<UserDevice> for DeviceInfo {
    fn from(device: UserDevice) -> Self {
        Self {
            id: device.id,
            name: device.name,
            public_key: device.public_key,
            address: device.address,
            state: device.state,
            created_at: device.created_at.timestamp(),
            updated_at: device.updated_at.timestamp(),
            revoked_at: device.revoked_at.map(|time| time.timestamp()),
        }
    }
}
