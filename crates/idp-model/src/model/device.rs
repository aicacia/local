#[cfg(not(feature = "std"))]
use alloc::string::String;

use chrono::{DateTime, Utc};

use super::Id;
use crate::contract::{DeviceInfo, DeviceState};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Device {
    pub id: Id,
    pub owner_subject: String,
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

#[cfg(test)]
mod tests {
    use super::Device;

    #[test]
    fn legacy_device_without_owner_is_rejected() {
        let device = r#"{"id":"00000000-0000-0000-0000-000000000001","name":"device","public_key":"key","address":"address","state":1,"created_at":1,"updated_at":1,"revoked_at":null}"#;
        assert!(serde_json::from_str::<Device>(device).is_err());
    }
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
