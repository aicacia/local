use serde::{Deserialize, Serialize};

use super::SetupStage;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct SetupStatus {
    pub stage: SetupStage,
}
