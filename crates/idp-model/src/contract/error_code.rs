#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

/// Standard OAuth 2.0 error codes (RFC 6749 Section 5.2).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    NotFound,
    UnauthorizedClient,
    AccessDenied,
    NotAuthorized,
    ServerError,
    TemporarilyUnavailable,
    InvalidGrant,
    InvalidClient,
    InvalidRequest,
    InvalidScope,
    InvalidResponseType,
    UnsupportedResponseType,
    UnsupportedGrantType,
    NotImplemented,
}
