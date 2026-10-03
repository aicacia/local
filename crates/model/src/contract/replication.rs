use serde::{Deserialize, Serialize};
#[cfg(feature = "utoipa")]
use utoipa::ToSchema;

pub const MANAGEMENT_REPLICATION_READ_SCOPE: &str = "management.replication.read";
pub const MANAGEMENT_REPLICATION_ADMIT_SCOPE: &str = "management.replication.admit";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "utoipa", derive(ToSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplicationAdmissionRequest {
    pub source_endpoint_id: String,
    pub target_endpoint_id: String,
    pub application_id: String,
    pub kind: String,
    pub resource_id: String,
    pub operation: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "utoipa", derive(ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SelectedResource {
    pub owner_subject: String,
    pub application_id: String,
    pub kind: String,
    pub resource_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "utoipa", derive(ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SelectedResourcesResponse {
    pub resources: Vec<SelectedResource>,
}
