#[cfg(not(feature = "std"))]
use alloc::string::String;

use serde::{Deserialize, Serialize};

use super::UserDeviceState;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DeviceEnrollment {
    pub id: i64,
    pub state: UserDeviceState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_code: Option<String>,
}
