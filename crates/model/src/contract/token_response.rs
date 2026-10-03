#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

use crate::contract::{AccessToken, IdToken, RefreshToken, TokenType};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
pub struct TokenResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_token: Option<IdToken>,
    /// The access token issued by the authorization server.
    pub access_token: AccessToken,
    /// The type of the token issued.
    pub token_type: TokenType,
    /// The lifetime in seconds of the id/access token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,
    /// The refresh token, which can be used to obtain new access tokens using the same authorization grant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<RefreshToken>,
    /// The lifetime in seconds of the refresh token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token_expires_in: Option<u64>,
    /// The scope of the access token as described by the authorization server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// The identifier of the authorization server that issued the token.
    #[serde(rename = "iss", skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
}
