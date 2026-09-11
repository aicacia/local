#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

/// Valid OAuth 2.0 response types for the authorization endpoint.
/// we do not support the `token` or `id_token` response type (implicit flow) as it is not recommended in OAuth 2.1 and has security weaknesses.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
pub enum ResponseType {
    /// Returns an authorization code.
    #[serde(rename = "code")]
    Code,
}
