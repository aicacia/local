#[cfg(not(feature = "std"))]
use alloc::string::String;

use serde::{Deserialize, Serialize};

use super::UserDeviceState;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: i64,
    pub name: String,
    pub public_key: String,
    pub address: String,
    pub state: UserDeviceState,
    pub created_at: i64,
    pub updated_at: i64,
    pub revoked_at: Option<i64>,
}
