#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use key::KeyError;
use serde::{Deserialize, Serialize};

use super::ErrorCode;

/// Standard OAuth 2.0 error response structure (RFC 6749 Section 5.2).
/// should match fields from crates/model/src/contract/error_response.rs
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
pub struct ErrorResponse {
    pub error: ErrorCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_uri: Option<String>,
}

impl From<serde::de::value::Error> for ErrorResponse {
    fn from(error: serde::de::value::Error) -> Self {
        ErrorResponse::new(ErrorCode::ServerError).with_description(error.to_string())
    }
}

impl From<KeyError> for ErrorResponse {
    fn from(error: KeyError) -> Self {
        ErrorResponse::new(ErrorCode::ServerError).with_description(error.to_string())
    }
}

impl ErrorResponse {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            error: code,
            error_description: None,
            error_uri: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.error_description = Some(description.into());
        self
    }

    pub fn with_error_uri(mut self, error_uri: impl Into<String>) -> Self {
        self.error_uri = Some(error_uri.into());
        self
    }
}

#[cfg(feature = "axum")]
impl axum::response::IntoResponse for ErrorResponse {
    fn into_response(self) -> axum::response::Response {
        let status = match self.error {
            ErrorCode::NotFound => axum::http::StatusCode::NOT_FOUND,
            ErrorCode::NotAuthorized => axum::http::StatusCode::UNAUTHORIZED,
            ErrorCode::InvalidRequest => axum::http::StatusCode::BAD_REQUEST,
            ErrorCode::InvalidClient => axum::http::StatusCode::BAD_REQUEST,
            ErrorCode::InvalidGrant => axum::http::StatusCode::BAD_REQUEST,
            ErrorCode::UnauthorizedClient => axum::http::StatusCode::FORBIDDEN,
            ErrorCode::UnsupportedGrantType => axum::http::StatusCode::BAD_REQUEST,
            ErrorCode::InvalidScope => axum::http::StatusCode::BAD_REQUEST,
            ErrorCode::ServerError => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::TemporarilyUnavailable => axum::http::StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::AccessDenied => axum::http::StatusCode::FORBIDDEN,
            ErrorCode::InvalidResponseType => axum::http::StatusCode::BAD_REQUEST,
            ErrorCode::UnsupportedResponseType => axum::http::StatusCode::BAD_REQUEST,
            ErrorCode::NotImplemented => axum::http::StatusCode::NOT_IMPLEMENTED,
        };
        (status, axum::Json(self)).into_response()
    }
}

pub type ErrorResponseResult<T> = Result<T, ErrorResponse>;
