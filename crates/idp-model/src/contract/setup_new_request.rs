#[cfg(not(feature = "std"))]
use alloc::string::String;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SetupNewRequest {
    pub device_name: String,
    pub admin_username: String,
    pub admin_password: String,
}
