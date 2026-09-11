#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

use super::ErrorResponse;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
#[serde(untagged)]
pub enum AuthorizationCodeResponse {
    Success {
        code: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        state: Option<String>,
        #[serde(rename = "iss", skip_serializing_if = "Option::is_none")]
        issuer: Option<String>,
    },
    Error {
        #[serde(flatten)]
        error: ErrorResponse,
        state: String,
    },
}
