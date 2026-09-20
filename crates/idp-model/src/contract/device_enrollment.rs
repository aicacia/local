#[cfg(not(feature = "std"))]
use alloc::string::String;

use serde::{Deserialize, Serialize};

use super::DeviceState;
use crate::model::Id;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DeviceEnrollment {
    #[cfg_attr(feature = "utoipa", schema(value_type = String))]
    pub id: Id,
    pub state: DeviceState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_code: Option<String>,
}
