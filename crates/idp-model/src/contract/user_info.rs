#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
pub struct UserInfo {
    pub sub: String,

    #[serde(default)]
    pub permissions: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub given_name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family_name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub middle_name: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_username: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email_verified: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone_number: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone_number_verified: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gender: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub birthdate: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zoneinfo: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,

    pub updated_at: DateTime<Utc>,
}
