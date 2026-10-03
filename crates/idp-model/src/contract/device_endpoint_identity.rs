use serde::{Deserialize, Serialize};

use crate::model::Id;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DeviceEndpointIdentity {
    #[cfg_attr(feature = "utoipa", schema(value_type = String))]
    pub device_id: Id,
    pub owner_subject: String,
    pub endpoint_id: String,
}
