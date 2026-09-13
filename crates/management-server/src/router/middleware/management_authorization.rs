use axum::extract::{FromRef, FromRequestParts};
use http::{HeaderValue, header::AUTHORIZATION, request::Parts};
use idp_model::contract::{ErrorCode, ErrorResponse};
use idp_service::oauth2::{Principal, decode_jwt};
use model::contract::StandardClaims;

use crate::RouterState;

pub const AUTHORIZATION_BEARER_PREFIX: &str = "Bearer ";

pub struct ManagementAuthorization {
    pub principal: Box<dyn Principal>,
}

impl ManagementAuthorization {
    pub fn new(principal: Box<dyn Principal>) -> Self {
        Self { principal }
    }
}

impl<S> FromRequestParts<S> for ManagementAuthorization
where
    RouterState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ErrorResponse;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if let Some(authorization_header_value) = parts.headers.get(AUTHORIZATION) {
            let authorization_string = authorization_from_header(authorization_header_value)?;
            let (jwt_header, _) = decode_jwt::<StandardClaims>(authorization_string)?;

            let router_state = RouterState::from_ref(state);
            if !router_state.global_identity_read_gate.verify().await {
                return Err(ErrorResponse::new(ErrorCode::NotAuthorized)
                    .with_description("global identity cache is not current"));
            }
            let principal = router_state
                .oauth2_service
                .find_principal(jwt_header.kid)
                .await?
                .ok_or_else(|| {
                    ErrorResponse::new(ErrorCode::NotAuthorized)
                        .with_description("principal not found for key id")
                })?;

            return Ok(Self::new(principal));
        }

        Err(ErrorResponse::new(ErrorCode::NotAuthorized)
            .with_description("missing authorization header"))
    }
}

fn authorization_from_header(
    authorization_header_value: &HeaderValue,
) -> Result<&str, ErrorResponse> {
    log::debug!("parsing authorization header");
    match authorization_header_value.to_str() {
        Ok(authorization_string) => {
            if authorization_string.len() < AUTHORIZATION_BEARER_PREFIX.len() {
                log::warn!(
                    "invalid authorization header is too short: length={}",
                    authorization_string.len()
                );
                return Err(ErrorResponse::new(ErrorCode::NotAuthorized)
                    .with_description("authorization header is too short"));
            }
            if !authorization_string.starts_with(AUTHORIZATION_BEARER_PREFIX) {
                log::warn!(
                    "authorization header does not start with 'Bearer ', starts with: {}",
                    authorization_string.chars().take(10).collect::<String>()
                );
                return Err(ErrorResponse::new(ErrorCode::NotAuthorized)
                    .with_description("authorization header does not start with 'Bearer '"));
            }
            log::debug!("authorization header parsed successfully");
            Ok(&authorization_string[AUTHORIZATION_BEARER_PREFIX.len()..])
        }
        Err(e) => {
            log::warn!(
                "invalid authorization header cannot be parsed as string: {}",
                e
            );
            Err(ErrorResponse::new(ErrorCode::NotAuthorized)
                .with_description("invalid authorization header cannot be parsed as string"))
        }
    }
}
