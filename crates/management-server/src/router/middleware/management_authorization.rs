use axum::extract::{FromRef, FromRequestParts};
use http::{HeaderValue, header::AUTHORIZATION, request::Parts};
use idp_model::contract::{EntityType, ErrorCode, ErrorResponse};
use idp_service::oauth2::{Principal, decode_jwt, verify_jwt};
use model::contract::{StandardClaims, TokenType, TokenUse};

use management_service::MANAGEMENT_APPLICATION_URI;

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
            let key_id = jwt_header
                .kid
                .parse::<idp_model::model::Id>()
                .map_err(|_| ErrorResponse::new(ErrorCode::NotAuthorized))?;
            let router_state = RouterState::from_ref(state);
            let principal = router_state
                .oauth2_service
                .find_principal(key_id)
                .await?
                .ok_or_else(|| {
                    ErrorResponse::new(ErrorCode::NotAuthorized)
                        .with_description("principal not found for key id")
                })?;
            let jwk = router_state.oauth2_service.find_public_jwk(key_id).await?;
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
                || claims.iat > now
                || claims.sub != principal.get_entity_id().to_string()
                || principal.get_entity_type() != EntityType::User
                || !management_audience(&claims)
            {
                return Err(ErrorResponse::new(ErrorCode::NotAuthorized)
                    .with_description("invalid bearer token claims"));
            }

            return Ok(Self::new(principal));
        }

        Err(ErrorResponse::new(ErrorCode::NotAuthorized)
            .with_description("missing authorization header"))
    }
}

fn management_audience(claims: &StandardClaims) -> bool {
    claims.aud == MANAGEMENT_APPLICATION_URI
}

#[cfg(test)]
mod tests {
    use model::contract::{StandardClaims, TokenType, TokenUse};

    use super::management_audience;

    #[test]
    fn storage_audience_does_not_authorize_management() {
        let mut claims = StandardClaims {
            r#type: TokenType::Bearer,
            r#use: TokenUse::Access,
            exp: i64::MAX,
            iat: 0,
            nbf: 0,
            iss: "issuer".into(),
            aud: "storage".into(),
            client_id: "client".into(),
            sub: "owner".into(),
            resource: Some("storage".into()),
            authorization_details: None,
            scope: vec!["storage".into()],
        };
        assert!(!management_audience(&claims));
        claims.aud = management_service::MANAGEMENT_APPLICATION_URI.into();
        assert!(management_audience(&claims));
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
