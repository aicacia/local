#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

use super::{CodeChallengeMethod, ResponseMode, ResponseType};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema, utoipa::IntoParams))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
pub struct AuthorizationRequest {
    pub response_type: ResponseType,
    pub client_id: String,

    /// redirect URI is optional if the client has only one registered redirect URI
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// Audience parameter (RFC 8707) to specify the intended recipients of the token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,

    /// PKCE parameters (RFC 7636) for public clients and enhanced security
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_challenge: Option<String>,
    /// The method used to derive the code challenge (e.g., "S256" or "plain").
    /// only required if `code_challenge` is present. "S256" is strongly recommended in OAuth 2.1 for better security.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_challenge_method: Option<CodeChallengeMethod>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_mode: Option<ResponseMode>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_token_hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_locales: Option<String>,
}
