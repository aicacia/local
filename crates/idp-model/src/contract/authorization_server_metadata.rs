#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

use super::{CodeChallengeMethod, GrantType, ResponseMode, ResponseType, TokenEndpointAuthMethod};

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
pub struct AuthorizationServerMetadata {
    /// REQUIRED. The authorization server's issuer identifier.
    /// URL using https scheme with no query or fragment.
    pub issuer: String,

    /// URL of the authorization endpoint.
    /// REQUIRED unless no grant types use it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_endpoint: Option<String>,

    /// URL of the token endpoint.
    /// REQUIRED in most cases (especially for OAuth 2.1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint: Option<String>,

    /// URL of the JWK Set document containing the server's public keys.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwks_uri: Option<String>,

    /// URL of the Dynamic Client Registration endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registration_endpoint: Option<String>,

    /// URL of the UserInfo endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub userinfo_endpoint: Option<String>,

    /// RECOMMENDED. List of supported OAuth 2.0 scope values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes_supported: Vec<String>,

    /// REQUIRED. List of supported response_type values.
    /// In OAuth 2.1 this should primarily be ["code"].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_types_supported: Vec<ResponseType>,

    /// List of supported response_mode values.
    /// Defaults to ["query", "fragment"] if omitted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_modes_supported: Vec<ResponseMode>,

    /// List of supported grant types.
    /// In OAuth 2.1: authorization_code, client_credentials, refresh_token (no implicit/password).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grant_types_supported: Vec<GrantType>,

    /// List of client authentication methods supported by the token endpoint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub token_endpoint_auth_methods_supported: Vec<TokenEndpointAuthMethod>,

    /// JWS signing algorithms supported for JWT client authentication
    /// at the token endpoint (private_key_jwt, client_secret_jwt).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub token_endpoint_auth_signing_alg_values_supported: Vec<String>,

    /// Human-readable service documentation URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_documentation: Option<String>,

    /// Languages and scripts supported for the user interface (BCP 47).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ui_locales_supported: Vec<String>,

    /// URL for the authorization server's policy on client use of data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op_policy_uri: Option<String>,

    /// URL for the authorization server's terms of service.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op_tos_uri: Option<String>,

    /// URL of the token revocation endpoint (RFC 7009).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revocation_endpoint: Option<String>,

    /// Client authentication methods supported by the revocation endpoint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revocation_endpoint_auth_methods_supported: Vec<TokenEndpointAuthMethod>,

    /// JWS signing algorithms supported for JWT auth at the revocation endpoint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub revocation_endpoint_auth_signing_alg_values_supported: Vec<String>,

    /// URL of the token introspection endpoint (RFC 7662).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub introspection_endpoint: Option<String>,

    /// Client authentication methods supported by the introspection endpoint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub introspection_endpoint_auth_methods_supported: Vec<TokenEndpointAuthMethod>,

    /// JWS signing algorithms supported for JWT auth at the introspection endpoint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub introspection_endpoint_auth_signing_alg_values_supported: Vec<String>,

    /// PKCE code challenge methods supported (S256 is strongly recommended in 2.1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub code_challenge_methods_supported: Vec<CodeChallengeMethod>,

    /// Signed metadata as a JWT (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signed_metadata: Option<String>,

    /// List of supported revocation endpoint authentication signing algorithms (additional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pushed_authorization_request_endpoint: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dpop_signing_alg_values_supported: Vec<String>,

    /// Whether the server requires PKCE for all clients (OAuth 2.1 best practice).
    #[serde(default)]
    pub require_pkce: bool,
}
