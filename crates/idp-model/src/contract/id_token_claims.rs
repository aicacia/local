#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use model::contract::StandardClaims;
use serde::{Deserialize, Serialize};

use crate::contract::UserInfo;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
pub struct IdTokenClaims {
    #[serde(flatten)]
    pub standard_claims: StandardClaims,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub user_info: Option<UserInfo>,
}
