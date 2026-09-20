#[cfg(not(feature = "std"))]
use alloc::string::String;

use serde::{Deserialize, Serialize};

use super::DeviceState;
use crate::model::Id;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    #[cfg_attr(feature = "utoipa", schema(value_type = String))]
    pub id: Id,
    pub name: String,
    pub public_key: String,
    pub address: String,
    pub state: DeviceState,
    pub created_at: i64,
    pub updated_at: i64,
    pub revoked_at: Option<i64>,
}
