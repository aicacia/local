use core::fmt;

#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

/// PKCE code challenge methods (RFC 7636).
/// the `plain` method is not included as it is not recommended in OAuth 2.1 due to security weaknesses. Only `S256` is supported for better security.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
pub enum CodeChallengeMethod {
    /// PKCE S256 method (SHA-256 hash of the code verifier).
    #[serde(rename = "S256")]
    S256,
}

impl fmt::Display for CodeChallengeMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodeChallengeMethod::S256 => write!(f, "S256"),
        }
    }
}
