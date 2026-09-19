#[cfg(not(feature = "std"))]
use alloc::string::String;

use chrono::{DateTime, Utc};

use super::Id;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OAuth2UserConsent {
    pub id: Id,

    pub user_id: Id,

    pub client_id: String,

    pub redirect_uri: String,

    pub scope: String,

    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub updated_at: DateTime<Utc>,
}
