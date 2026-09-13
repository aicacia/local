use std::marker::PhantomData;

use axum::extract::{FromRef, FromRequestParts};
use http::{HeaderValue, header::AUTHORIZATION, request::Parts};
use idp_model::contract::EntityType;
use idp_model::contract::{ErrorCode, ErrorResponse};
use idp_service::oauth2::{Principal, decode_jwt, verify_jwt};
use model::contract::{StandardClaims, TokenType, TokenUse};
use serde::de::DeserializeOwned;

use crate::RouterState;

pub const AUTHORIZATION_BEARER_PREFIX: &str = "Bearer ";

pub type StandardAuthorization = Authorization<StandardClaims>;

pub struct Authorization<T>
where
    T: DeserializeOwned + Send,
{
    pub principal: Box<dyn Principal>,
    pub claims: T,
    pub token: String,
    _phantom_data: PhantomData<T>,
}

impl<S> FromRequestParts<S> for Authorization<StandardClaims>
where
    RouterState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = ErrorResponse;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if let Some(authorization_header_value) = parts.headers.get(AUTHORIZATION) {
            let authorization_string = authorization_from_header(authorization_header_value)?;
            let router_state = RouterState::from_ref(state);
            let authorization = authorize_bearer(&router_state, authorization_string).await?;
            return Ok(Authorization {
                principal: authorization.principal,
                claims: authorization.claims,
                token: authorization.token,
                _phantom_data: PhantomData,
            });
        }
        Err(ErrorResponse::new(ErrorCode::NotAuthorized)
            .with_description("missing authorization header"))
    }
}

pub async fn require_current_global_identity(
    router_state: &RouterState,
) -> Result<(), ErrorResponse> {
    if router_state.global_identity_is_current().await {
        Ok(())
    } else {
        Err(ErrorResponse::new(ErrorCode::NotAuthorized)
            .with_description("global identity cache is not current"))
    }
}

pub async fn authorize_bearer(
    router_state: &RouterState,
    authorization_string: &str,
) -> Result<Authorization<StandardClaims>, ErrorResponse> {
    require_current_global_identity(router_state).await?;
    let (jwt_header, _) = decode_jwt::<StandardClaims>(authorization_string)?;
    let principal = router_state
        .oauth2_service
        .find_principal(jwt_header.kid)
        .await?
        .ok_or_else(|| {
            ErrorResponse::new(ErrorCode::NotAuthorized)
                .with_description("principal not found for key id")
        })?;
    let jwk = router_state
        .oauth2_service
        .find_public_jwk(jwt_header.kid)
        .await?;
    let (_, claims) = verify_jwt::<StandardClaims>(&jwk, authorization_string)?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| ErrorResponse::new(ErrorCode::NotAuthorized))?
        .as_secs() as i64;
    if claims.r#type != TokenType::Bearer
        || claims.r#use != TokenUse::Access
        || claims.iss != router_state.oauth2_service.metadata().issuer
        || claims.exp <= now
        || claims.nbf > now
        || principal.get_entity_type() != EntityType::User
        || claims.sub != principal.get_entity_id().to_string()
        || claims.aud.is_empty()
    {
        return Err(ErrorResponse::new(ErrorCode::NotAuthorized)
            .with_description("invalid bearer token claims"));
    }

    Ok(Authorization {
        principal,
        claims,
        token: authorization_string.to_owned(),
        _phantom_data: PhantomData,
    })
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
