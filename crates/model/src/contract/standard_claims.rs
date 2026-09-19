#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

use crate::contract::{AuthorizationDetail, TokenType, TokenUse};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
pub struct StandardClaims {
    pub r#type: TokenType,
    pub r#use: TokenUse,
    pub exp: i64,
    pub iat: i64,
    pub nbf: i64,
    pub iss: String,
    pub aud: String,
    pub client_id: String,
    pub sub: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_details: Option<Vec<AuthorizationDetail>>,
    #[serde(
        serialize_with = "crate::space_delimited::serialize",
        deserialize_with = "crate::space_delimited::deserialize"
    )]
    pub scope: Vec<String>,
}
