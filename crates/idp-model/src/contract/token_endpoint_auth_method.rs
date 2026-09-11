use core::fmt;

#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

/// Client token endpoint authentication methods (RFC 7591 / OIDC).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
#[serde(rename_all = "snake_case")]
pub enum TokenEndpointAuthMethod {
    /// Client authenticates using the HTTP Basic authentication scheme.
    ClientSecretBasic,
    /// Client authenticates by including the credentials in the request body.
    ClientSecretPost,
    /// Client authenticates using a signed JWT assertion (Asymmetric Crypto).
    PrivateKeyJwt,
    /// Client authenticates using a signed JWT utilizing the shared client secret.
    ClientSecretJwt,
    /// Public client that does not authenticate at the token endpoint (e.g., PKCE only).
    None,
}

impl fmt::Display for TokenEndpointAuthMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenEndpointAuthMethod::ClientSecretBasic => write!(f, "client_secret_basic"),
            TokenEndpointAuthMethod::ClientSecretPost => write!(f, "client_secret_post"),
            TokenEndpointAuthMethod::PrivateKeyJwt => write!(f, "private_key_jwt"),
            TokenEndpointAuthMethod::ClientSecretJwt => write!(f, "client_secret_jwt"),
            TokenEndpointAuthMethod::None => write!(f, "none"),
        }
    }
}
