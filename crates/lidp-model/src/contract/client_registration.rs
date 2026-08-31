#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

use crate::contract::ApplicationRegistration;

use super::{ClientProfile, ClientType, GrantType, ResponseType, TokenEndpointAuthMethod};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
pub struct ClientRegistration {
    /// Unique identifier for the application (assigned by the authorization server).
    pub application: ApplicationRegistration,

    /// Unique identifier for the client application (assigned by the authorization server).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,

    /// Optional or required depending on registration type (Public vs Confidential).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,

    /// Time at which the client identifier was issued (Unix timestamp).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id_issued_at: Option<i64>,

    /// Time at which the client secret will expire (0 or None for no expiration).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret_expires_at: Option<i64>,

    /// Human-readable name of the client application.
    pub client_name: String,

    /// URL of the home page of the client.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_uri: Option<String>,

    /// URL that references a logo for the client application.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo_uri: Option<String>,

    /// Array of email addresses for people responsible for this client.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contacts: Vec<String>,

    /// URL for the application's terms of service.
    #[serde(rename = "tos_uri", skip_serializing_if = "Option::is_none")]
    pub terms_of_service_uri: Option<String>,

    /// URL for the application's privacy policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_uri: Option<String>,

    #[serde(default)]
    pub client_type: ClientType,
    pub profile: ClientProfile,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redirect_uris: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_grant_types: Vec<GrantType>,

    /// Standard OAuth 2.0 response types (e.g., `["code"]`, `["token"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_types: Vec<ResponseType>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_scopes: Vec<String>,

    /// Requested authentication method for the token endpoint
    /// (e.g., `client_secret_basic`, `client_secret_post`, `private_key_jwt`).
    pub token_endpoint_auth_method: TokenEndpointAuthMethod,

    // ==========================================
    // 5. Software Statements & Provenance
    // ==========================================
    /// A digitally signed JWT assertion containing verifiable client metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub software_statement: Option<String>,

    /// A unique identifier string assigned by the software issuer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub software_id: Option<String>,

    /// A version identifier string string assigned by the software issuer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub software_version: Option<String>,
}
