use serde::{Deserialize, Serialize};

use crate::model::Id;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct StorageSession {
    pub token: String,
    pub expires_at: i64,
    pub application_id: Id,
}
