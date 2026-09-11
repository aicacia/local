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
#[serde(rename_all = "snake_case")]
pub enum GrantType {
    /// Password Grant (Used for exchanging user credentials for access tokens)
    Password,
    /// Authorization Code Grant (Best practice for apps with a backend)
    AuthorizationCode,
    /// Client Credentials Grant (Machine-to-Machine)
    ClientCredentials,
    /// Refresh Token Grant (To exchange for new access tokens)
    RefreshToken,
}
