#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
pub enum ResponseMode {
    /// Return parameters in the query string (default for `code` response type).
    #[serde(rename = "query")]
    Query,
    /// Return parameters as form-encoded body (for `code` response type).
    #[serde(rename = "form_post")]
    FormPost,
}
