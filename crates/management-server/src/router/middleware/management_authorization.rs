use axum::extract::{FromRef, FromRequestParts};
use http::{HeaderValue, header::AUTHORIZATION, request::Parts};
use idp_model::{
    contract::{ErrorCode, ErrorResponse},
    model::Id,
};
use model::contract::StandardClaims;

use crate::RouterState;

pub const AUTHORIZATION_BEARER_PREFIX: &str = "Bearer ";

pub struct ManagementAuthorization {
    pub subject: Id,
    pub(crate) application_id: Id,
}

impl ManagementAuthorization {
    pub fn new(subject: Id, application_id: Id) -> Self {
        Self {
            subject,
            application_id,
        }
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
            let router_state = RouterState::from_ref(state);
            let (claims, application_id) = router_state
                .control_plane
                .validate_actor_token(authorization_string)
                .await
                .map_err(|error| {
                    if error == "actor token was rejected by IdP" {
                        ErrorResponse::new(ErrorCode::NotAuthorized)
                    } else {
                        ErrorResponse::new(ErrorCode::TemporarilyUnavailable)
                    }
                })?;
            if !management_audience(&claims) {
                return Err(ErrorResponse::new(ErrorCode::NotAuthorized));
            }
            let subject = claims
                .sub
                .parse::<Id>()
                .map_err(|_| ErrorResponse::new(ErrorCode::NotAuthorized))?;
            return Ok(Self::new(subject, application_id));
        }

        Err(ErrorResponse::new(ErrorCode::NotAuthorized)
            .with_description("missing authorization header"))
    }
}

fn management_audience(claims: &StandardClaims) -> bool {
    claims.principal_type == model::contract::PrincipalType::User
        && claims.aud == management_service::MANAGEMENT_APPLICATION_URI
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

#[cfg(test)]
mod tests {
    use model::contract::{PrincipalType, StandardClaims, TokenType, TokenUse};

    use super::management_audience;

    #[test]
    fn only_user_principals_with_management_audience_are_accepted() {
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
            principal_type: PrincipalType::User,
            resource: Some("storage".into()),
            authorization_details: None,
            scope: vec!["storage".into()],
        };
        assert!(!management_audience(&claims));
        claims.aud = management_service::MANAGEMENT_APPLICATION_URI.into();
        assert!(management_audience(&claims));
        claims.principal_type = PrincipalType::Client;
        assert!(!management_audience(&claims));
    }
}
