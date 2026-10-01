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
    #[serde(rename = "urn:ietf:params:oauth:grant-type:token-exchange")]
    TokenExchange,
}

#[cfg(test)]
mod tests {
    use super::GrantType;

    #[test]
    fn serializes_token_exchange_grant_type_as_its_oauth_urn() {
        assert_eq!(
            serde_json::to_string(&GrantType::TokenExchange)
                .expect("serialize token exchange grant type"),
            "\"urn:ietf:params:oauth:grant-type:token-exchange\""
        );
    }
}
