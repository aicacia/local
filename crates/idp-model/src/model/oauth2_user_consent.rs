#[cfg(not(feature = "std"))]
use alloc::string::String;

use chrono::{DateTime, Utc};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OAuth2UserConsent {
    pub id: i64,

    pub user_id: i64,

    pub client_id: String,

    pub redirect_uri: String,

    pub scope: String,

    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub updated_at: DateTime<Utc>,
}
