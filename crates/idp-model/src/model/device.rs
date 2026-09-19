#[cfg(not(feature = "std"))]
use alloc::string::String;

use chrono::{DateTime, Utc};

use super::Id;
use crate::contract::{DeviceInfo, DeviceState};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Device {
    pub id: Id,
    pub name: String,
    pub public_key: String,
    pub address: String,
    #[serde(with = "super::sql_enum::device_state")]
    pub state: DeviceState,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub updated_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub revoked_at: Option<DateTime<Utc>>,
}

impl From<Device> for DeviceInfo {
    fn from(device: Device) -> Self {
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
