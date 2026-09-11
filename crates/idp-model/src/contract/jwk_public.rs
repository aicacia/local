#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

use super::{JwkPublicParameters, JwsAlgorithm, KeyUse};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
pub struct JwkPublic {
    /// Key use (sig, enc)
    #[serde(rename = "use")]
    pub r#use: KeyUse,

    /// Key ID
    pub kid: u32,

    /// Algorithm intended for use
    pub alg: JwsAlgorithm,

    #[serde(flatten)]
    pub params: JwkPublicParameters,
}
