#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

use crate::contract::PasswordGrantRequest;

use super::{
    AuthorizationCodeGrantRequest, ClientCredentialsGrantRequest, RefreshTokenGrantRequest,
    TokenExchangeGrantRequest,
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
#[serde(tag = "grant_type")]
pub enum TokenRequest {
    #[serde(rename = "password")]
    Password(PasswordGrantRequest),
    #[serde(rename = "authorization_code")]
    AuthorizationCode(AuthorizationCodeGrantRequest),
    #[serde(rename = "client_credentials")]
    ClientCredentials(ClientCredentialsGrantRequest),
    #[serde(rename = "refresh_token")]
    RefreshToken(RefreshTokenGrantRequest),
    #[serde(rename = "urn:ietf:params:oauth:grant-type:token-exchange")]
    TokenExchange(TokenExchangeGrantRequest),
}
