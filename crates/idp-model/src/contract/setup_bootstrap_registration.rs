#[cfg(not(feature = "std"))]
use alloc::string::String;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SetupBootstrapRegistration {
    pub grant: String,
    pub endpoint_id: String,
    pub endpoint_addr: String,
    pub expires_at: i64,
}
