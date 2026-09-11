#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};

use chrono::{DateTime, Utc};

use crate::contract::CodeChallengeMethod;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct OAuth2AuthorizationCode {
    pub id: i64,

    pub code: String,

    pub client_id: String,

    pub key_id: u32,

    pub redirect_uri: String,

    #[serde(with = "model::json_vec")]
    pub scopes: Vec<String>,

    pub resource: Option<String>,

    pub code_challenge: Option<String>,

    #[serde(with = "super::sql_enum::code_challenge_method_option")]
    pub code_challenge_method: Option<CodeChallengeMethod>,

    pub nonce: Option<String>,

    #[serde(with = "chrono::serde::ts_seconds")]
    pub expires_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub consumed_at: Option<DateTime<Utc>>,

    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub updated_at: DateTime<Utc>,
}
