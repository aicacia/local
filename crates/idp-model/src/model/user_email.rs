#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use chrono::{DateTime, Utc};

use super::Id;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UserEmail {
    pub id: Id,

    pub user_id: Id,

    pub email: String,

    pub verified: bool,

    /// Only one email per user should be primary.
    pub primary: bool,

    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub updated_at: DateTime<Utc>,
}
