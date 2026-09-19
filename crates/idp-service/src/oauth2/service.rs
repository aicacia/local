#[cfg(not(feature = "std"))]
use alloc::{
    format,
    string::{String, ToString},
    sync::Arc,
    vec,
    vec::Vec,
};
use core::future::Future;

use model::contract::{
    AccessToken, AuthorizationDetail, IdToken, RefreshToken, StandardClaims, TokenResponse,
    TokenType, TokenUse,
};
#[cfg(feature = "std")]
use std::sync::Arc;

use chrono::{Duration, Utc};
use idp_model::model::{Client, Key};
use idp_model::{
    contract::{
        ApproveForUserRequest, AuthorizationCodeResponse, AuthorizationRequest,
        AuthorizationServerMetadata, ClientRegistration, ClientType, DeviceAuthorization,
        DeviceAuthorizationRequest, EntityType, ErrorCode, ErrorResponse, ErrorResponseResult,
        GrantType, IdTokenClaims, IsAllowedForUserRequest, IsAllowedForUserResponse, JwkPrivate,
        JwkPublic, Jwks, OAuth2ClientAuth, RevocationRequest, SubjectTokenType, TokenRequest,
        TunnelAuthorization, TunnelAuthorizationClaims, TunnelAuthorizationRequest, UserInfo,
    },
    model::User,
};

use crate::{
    oauth2::{Principal, UserPrincipal, decode_jwt, encode_jwt, verify_jwt},
    repo::{
        ApplicationRepo, ClientRepo, KeyRepo, KeyService, OAuth2AuthorizationCodeRepo,
        OAuth2UserConsentRepo, PrivateKeyRepo, UserRepo,
    },
    util::{generate_random_string, verify_password},
};

use super::{
    OAuth2Config, intersect_scopes, parse_scopes, resolve_redirect_uri,
    validate_authorization_code_grant, validate_authorization_details,
    validate_authorization_request, validate_scopes, verify_code_challenge,
};

pub trait TokenIssuerAuthorizer: Send + Sync {
    fn authorize<'a>(
        &'a self,
        client: &'a Client,
        subject: &'a str,
        authorization_details: &'a [AuthorizationDetail],
    ) -> impl Future<Output = ErrorResponseResult<()>> + Send + 'a;
}

pub struct OAuth2Service<A, C, AC, U, G, K> {
    pub application_repo: A,
    pub client_repo: C,
    pub authorization_code_repo: AC,
    pub user_repo: U,
    pub oauth2_user_consent_repo: G,
    pub key_service: Arc<KeyService<K>>,
    pub oauth_config: OAuth2Config,
}

#[derive(Clone, Debug)]
pub struct UpdateUserInfoRequest {
    pub name: Option<String>,
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub middle_name: Option<String>,
    pub nickname: Option<String>,
    pub profile: Option<String>,
    pub picture: Option<String>,
    pub website: Option<String>,
    pub sex: Option<String>,
    pub birthdate: Option<String>,
    pub zoneinfo: Option<String>,
    pub locale: Option<String>,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub phone_number: Option<String>,
    pub phone_number_verified: Option<bool>,
}

const TUNNEL_AUTHORIZATION_TTL: Duration = Duration::seconds(60);

impl<A, C, AC, U, G, K> OAuth2Service<A, C, AC, U, G, K>
where
    A: ApplicationRepo,
    C: ClientRepo,
    AC: OAuth2AuthorizationCodeRepo,
    U: UserRepo,
    G: OAuth2UserConsentRepo,
    K: KeyRepo,
{
    pub fn new(
        application_repo: A,
        client_repo: C,
        authorization_code_repo: AC,
        user_repo: U,
        oauth2_user_consent_repo: G,
        key_service: Arc<KeyService<K>>,
        oauth_config: OAuth2Config,
    ) -> Self {
        Self {
            application_repo,
            client_repo,
            authorization_code_repo,
            user_repo,
            oauth2_user_consent_repo,
            key_service,
            oauth_config,
        }
    }

    pub async fn register_client(
        &self,
        request: ClientRegistration,
    ) -> ErrorResponseResult<ClientRegistration> {
        let client = ClientRegistration {
            client_id: Some(
                request
                    .client_id
                    .unwrap_or_else(generate_random_string::<32>),
            ),
            client_secret: Some(
                request
                    .client_secret
                    .unwrap_or_else(generate_random_string::<32>),
            ),
            ..request
        };

        let client = self
            .client_repo
            .create_client(client)
            .await
            .map_err(ErrorResponse::from)?;

        Ok(client.into())
    }

    pub async fn get_client(&self, client_id: &str) -> ErrorResponseResult<ClientRegistration> {
        let client = self
            .client_repo
            .find_client_by_client_id(client_id)
            .await
            .map_err(ErrorResponse::from)?
            .ok_or_else(|| {
                ErrorResponse::new(ErrorCode::InvalidClient).with_description("client not found")
            })?;
        Ok(client.into())
    }

    pub async fn application_id_for_client(&self, client_id: &str) -> ErrorResponseResult<i64> {
        self.client_repo
            .find_client_by_client_id(client_id)
            .await
            .map_err(ErrorResponse::from)?
            .map(|client| client.application_id)
            .ok_or_else(|| ErrorResponse::new(ErrorCode::InvalidClient))
    }

    pub async fn list_clients(
        &self,
        offset: u32,
        limit: u32,
    ) -> ErrorResponseResult<Vec<ClientRegistration>> {
        let clients = self
            .client_repo
            .list_clients(offset, limit)
            .await
            .map_err(ErrorResponse::from)?;

        Ok(clients.into_iter().map(Into::into).collect())
    }

    pub async fn update_client(
        &self,
        client_id: &str,
        request: ClientRegistration,
    ) -> ErrorResponseResult<ClientRegistration> {
        let existing = self
            .client_repo
            .find_client_by_client_id(client_id)
            .await
            .map_err(ErrorResponse::from)?
            .ok_or_else(|| {
                ErrorResponse::new(ErrorCode::InvalidClient).with_description("client not found")
            })?;

        let now = Utc::now();
        let client = Client {
            id: existing.id,
            application_id: existing.application_id,
            client_id: existing.client_id,
            client_secret: request.client_secret.unwrap_or(existing.client_secret),
            client_id_issued_at: existing.client_id_issued_at,
            client_secret_expires_at: request
                .client_secret_expires_at
                .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0))
                .or(existing.client_secret_expires_at),
            client_name: request.client_name,
            client_uri: request.client_uri.unwrap_or(existing.client_uri),
            redirect_uris: request.redirect_uris,
            client_type: request.client_type,
            profile: request.profile,
            token_endpoint_auth_method: request.token_endpoint_auth_method,
            allowed_grant_types: request.allowed_grant_types,
            response_types: request.response_types,
            allowed_scopes: request.allowed_scopes,
            logo_uri: request.logo_uri,
            contacts: request.contacts,
            terms_of_service_uri: request.terms_of_service_uri,
            policy_uri: request.policy_uri,
            software_statement: request.software_statement,
            software_id: request.software_id,
            software_version: request.software_version,
            created_at: existing.created_at,
            updated_at: now,
        };

        let client = self
            .client_repo
            .update_client(client)
            .await
            .map_err(ErrorResponse::from)?;
        Ok(client.into())
    }

    pub async fn delete_client(&self, client_id: &str) -> ErrorResponseResult<()> {
        self.client_repo
            .delete_client_by_client_id(client_id)
            .await
            .map_err(ErrorResponse::from)
    }

    pub async fn authorize<PT: Principal + ?Sized>(
        &self,
        request: AuthorizationRequest,
        principal: &PT,
    ) -> ErrorResponseResult<AuthorizationCodeResponse> {
        let client = self
            .client_repo
            .find_client_by_client_id(&request.client_id)
            .await
            .map_err(ErrorResponse::from)?
            .ok_or_else(|| {
                ErrorResponse::new(ErrorCode::InvalidClient).with_description("client not found")
            })?;

        validate_authorization_request(&request, &client, self.oauth_config.require_pkce)?;
        let redirect_uri = resolve_redirect_uri(&request, &client)?;
        let requested_scopes = request
            .scope
            .as_deref()
            .map(parse_scopes)
            .unwrap_or_default();
        let scopes = intersect_scopes(&requested_scopes, &client.allowed_scopes);
        let normalized_scope = Self::normalize_scopes(&scopes);

        if principal.get_entity_type() != EntityType::User {
            return Err(ErrorResponse::new(ErrorCode::AccessDenied)
                .with_description("only users can authorize clients"));
        }

        let consent = self
            .oauth2_user_consent_repo
            .find_user_consent(
                principal.get_entity_id(),
                &client.client_id,
                &redirect_uri,
                &normalized_scope,
            )
            .await
            .map_err(ErrorResponse::from)?;

        if consent.is_none() {
            return Err(ErrorResponse::new(ErrorCode::AccessDenied)
                .with_description("client approval required"));
        }

        let authorization_code = self
            .authorization_code_repo
            .create_authorization_code(
                client.client_id,
                principal.get_key().id,
                redirect_uri,
                scopes,
                request.resource,
                request.code_challenge,
                request.code_challenge_method,
                request.nonce,
                Utc::now()
                    + Duration::seconds(self.oauth_config.authorization_code_ttl_secs as i64),
            )
            .await
            .map_err(ErrorResponse::from)?;

        Ok(AuthorizationCodeResponse::Success {
            code: authorization_code.code,
            state: request.state,
            issuer: Some(self.oauth_config.issuer.clone()),
        })
    }

    pub async fn approve_for_user<PT: Principal + ?Sized>(
        &self,
        request: ApproveForUserRequest,
        principal: &PT,
    ) -> ErrorResponseResult<IsAllowedForUserResponse> {
        if principal.get_entity_type() != EntityType::User {
            return Err(ErrorResponse::new(ErrorCode::AccessDenied)
                .with_description("only users can approve clients"));
        }

        let client = self
            .client_repo
            .find_client_by_client_id(&request.client_id)
            .await
            .map_err(ErrorResponse::from)?
            .ok_or_else(|| {
                ErrorResponse::new(ErrorCode::InvalidClient).with_description("client not found")
            })?;

        if !client.redirect_uris.contains(&request.redirect_uri) {
            return Err(ErrorResponse::new(ErrorCode::InvalidRequest)
                .with_description("redirect_uri is not allowed for this client"));
        }

        let requested_scopes = parse_scopes(&request.scope);
        validate_scopes(&requested_scopes, &client.allowed_scopes)?;
        let effective_scopes = intersect_scopes(&requested_scopes, &client.allowed_scopes);
        let normalized_scope = Self::normalize_scopes(&effective_scopes);

        self.oauth2_user_consent_repo
            .upsert_user_consent(
                principal.get_entity_id(),
                &client.client_id,
                &request.redirect_uri,
                &normalized_scope,
            )
            .await
            .map_err(ErrorResponse::from)?;

        Ok(IsAllowedForUserResponse { allowed: true })
    }

    pub async fn is_allowed_for_user<PT: Principal + ?Sized>(
        &self,
        request: IsAllowedForUserRequest,
        principal: &PT,
    ) -> ErrorResponseResult<IsAllowedForUserResponse> {
        if principal.get_entity_type() != EntityType::User {
            return Err(ErrorResponse::new(ErrorCode::AccessDenied)
                .with_description("only users can check client approval"));
        }

        let client = self
            .client_repo
            .find_client_by_client_id(&request.client_id)
            .await
            .map_err(ErrorResponse::from)?
            .ok_or_else(|| {
                ErrorResponse::new(ErrorCode::InvalidClient).with_description("client not found")
            })?;

        if !client.redirect_uris.contains(&request.redirect_uri) {
            return Ok(IsAllowedForUserResponse { allowed: false });
        }

        let requested_scopes = parse_scopes(&request.scope);
        validate_scopes(&requested_scopes, &client.allowed_scopes)?;
        let effective_scopes = intersect_scopes(&requested_scopes, &client.allowed_scopes);
        let normalized_scope = Self::normalize_scopes(&effective_scopes);

        let consent = self
            .oauth2_user_consent_repo
            .find_user_consent(
                principal.get_entity_id(),
                &client.client_id,
                &request.redirect_uri,
                &normalized_scope,
            )
            .await
            .map_err(ErrorResponse::from)?;

        Ok(IsAllowedForUserResponse {
            allowed: consent.is_some(),
        })
    }

    pub async fn token_with_authorizer<T>(
        &self,
        request: TokenRequest,
        client_auth: Option<OAuth2ClientAuth>,
        authorizer: Option<&T>,
    ) -> ErrorResponseResult<TokenResponse>
    where
        T: TokenIssuerAuthorizer,
    {
        match request {
            TokenRequest::Password(request) => {
                let client = self
                    .client_repo
                    .find_client_by_client_id(&request.client_id)
                    .await
                    .map_err(ErrorResponse::from)?
                    .ok_or_else(|| {
                        ErrorResponse::new(ErrorCode::InvalidClient)
                            .with_description(format!("client {} not found", request.client_id))
                    })?;
                self.validate_grant_type(&client, GrantType::Password)?;
                self.authenticate_client_for_token_endpoint(&client, client_auth.as_ref())?;

                let user = self
                    .user_repo
                    .find_user_by_username_or_email(&request.username)
                    .await
                    .map_err(ErrorResponse::from)?
                    .ok_or_else(|| {
                        ErrorResponse::new(ErrorCode::InvalidGrant)
                            .with_description(format!("user {} not found", request.username))
                    })?;

                let user_password = self
                    .user_repo
                    .find_user_password_by_user_id(user.id)
                    .await
                    .map_err(ErrorResponse::from)?
                    .ok_or_else(|| {
                        ErrorResponse::new(ErrorCode::InvalidGrant)
                            .with_description("user password not found")
                    })?;

                let verified_password = verify_password(
                    &request.password,
                    &user_password.password_hash,
                )
                .map_err(|e| {
                    ErrorResponse::new(ErrorCode::ServerError).with_description(e.to_string())
                })?;

                if !verified_password {
                    return Err(ErrorResponse::new(ErrorCode::InvalidGrant)
                        .with_description("invalid username or password"));
                }

                let key = self
                    .key_service
                    .key_repo()
                    .find_by_entity_type_and_id(EntityType::User, user.id)
                    .await
                    .map_err(ErrorResponse::from)?
                    .ok_or_else(|| {
                        ErrorResponse::new(ErrorCode::InvalidGrant)
                            .with_description("user key not found")
                    })?;

                let principal = self.find_principal(key.id).await?.ok_or_else(|| {
                    ErrorResponse::new(ErrorCode::InvalidGrant)
                        .with_description("principal not found for user")
                })?;

                let requested_scopes = request
                    .scope
                    .as_deref()
                    .map(parse_scopes)
                    .unwrap_or_default();
                let scopes = intersect_scopes(&requested_scopes, &client.allowed_scopes);

                self.issue_tokens_for_client(
                    &client,
                    principal.as_ref(),
                    &scopes,
                    request.resource.as_deref(),
                    None,
                    None,
                    authorizer,
                )
                .await
            }
            TokenRequest::AuthorizationCode(request) => {
                let now = Utc::now();
                let authorization_code = self
                    .authorization_code_repo
                    .find_authorization_code_by_code(&request.code)
                    .await
                    .map_err(ErrorResponse::from)?
                    .ok_or_else(|| {
                        ErrorResponse::new(ErrorCode::InvalidGrant)
                            .with_description("authorization code not found")
                    })?;

                if authorization_code.consumed_at.is_some() {
                    return Err(ErrorResponse::new(ErrorCode::InvalidGrant)
                        .with_description("authorization code already consumed"));
                }

                if authorization_code.expires_at < now {
                    return Err(ErrorResponse::new(ErrorCode::InvalidGrant)
                        .with_description("authorization code expired"));
                }

                let principal = self
                    .find_principal(authorization_code.key_id)
                    .await?
                    .ok_or_else(|| {
                        ErrorResponse::new(ErrorCode::InvalidGrant)
                            .with_description("principal not found for authorization code")
                    })?;

                let client = self
                    .client_repo
                    .find_client_by_client_id(&authorization_code.client_id)
                    .await
                    .map_err(ErrorResponse::from)?
                    .ok_or_else(|| {
                        ErrorResponse::new(ErrorCode::InvalidClient)
                            .with_description("client not found")
                    })?;
                self.validate_grant_type(&client, GrantType::AuthorizationCode)?;
                self.authenticate_client_for_token_endpoint(&client, client_auth.as_ref())?;

                validate_authorization_code_grant(
                    &request,
                    &authorization_code.client_id,
                    Some(&authorization_code.redirect_uri),
                )?;

                if let Some(code_challenge) = &authorization_code.code_challenge {
                    verify_code_challenge(
                        &request.code_verifier,
                        code_challenge,
                        authorization_code.code_challenge_method.ok_or_else(|| {
                            ErrorResponse::new(ErrorCode::InvalidGrant).with_description(
                                "code_challenge_method is required when code_challenge is present",
                            )
                        })?,
                    )?;
                }

                self.authorization_code_repo
                    .consume_authorization_code(authorization_code.id, now)
                    .await
                    .map_err(ErrorResponse::from)?;

                self.issue_tokens_for_client(
                    &client,
                    principal.as_ref(),
                    &authorization_code.scopes,
                    authorization_code.resource.as_deref(),
                    None,
                    None,
                    authorizer,
                )
                .await
            }
            TokenRequest::ClientCredentials(request) => {
                let auth = client_auth.as_ref().ok_or_else(|| {
                    ErrorResponse::new(ErrorCode::InvalidClient)
                        .with_description("client authentication is required")
                })?;

                let client = self
                    .client_repo
                    .find_client_by_client_id(&auth.client_id)
                    .await
                    .map_err(ErrorResponse::from)?
                    .ok_or_else(|| {
                        ErrorResponse::new(ErrorCode::InvalidClient)
                            .with_description("client not found")
                    })?;
                self.validate_grant_type(&client, GrantType::ClientCredentials)?;
                self.authenticate_client_for_token_endpoint(&client, Some(auth))?;

                let requested_scopes = request
                    .scope
                    .as_deref()
                    .map(parse_scopes)
                    .unwrap_or_default();
                let _scopes = intersect_scopes(&requested_scopes, &client.allowed_scopes);

                // we need a service account principal for the client credentials grant, but we don't have that implemented yet, so we'll just return an error for now
                // self.issue_tokens_for_client(
                //     &client,
                //     &principal,
                //     &scopes,
                //     request.resource.as_deref(),
                // )
                // .await
                Err(ErrorResponse::new(ErrorCode::UnsupportedGrantType)
                    .with_description("client credentials grant is not implemented"))
            }
            TokenRequest::RefreshToken(request) => {
                let now = Utc::now();

                let (jwt_header, refresh_token) =
                    decode_jwt::<StandardClaims>(&request.refresh_token.0)?;

                if refresh_token.r#use != TokenUse::Refresh {
                    return Err(ErrorResponse::new(ErrorCode::InvalidGrant)
                        .with_description("token is not a refresh token"));
                }
                if refresh_token.exp < now.timestamp() {
                    return Err(ErrorResponse::new(ErrorCode::InvalidGrant)
                        .with_description("refresh token is expired"));
                }
                let key = self
                    .key_service
                    .key_repo()
                    .find_by_id(jwt_header.kid)
                    .await?
                    .ok_or_else(|| {
                        ErrorResponse::new(ErrorCode::InvalidGrant)
                            .with_description("refresh token signing key not found")
                    })?;

                // TODO: get a derevided key from the key ring store to validate token.

                let client = self
                    .client_repo
                    .find_client_by_client_id(&refresh_token.client_id)
                    .await
                    .map_err(ErrorResponse::from)?
                    .ok_or_else(|| {
                        ErrorResponse::new(ErrorCode::InvalidClient)
                            .with_description("client not found")
                    })?;
                self.validate_grant_type(&client, GrantType::RefreshToken)?;
                self.authenticate_client_for_token_endpoint(&client, client_auth.as_ref())?;

                let requested_scopes = request
                    .scope
                    .as_deref()
                    .map(parse_scopes)
                    .unwrap_or_default();
                let scopes = if requested_scopes.is_empty() {
                    refresh_token.scope
                } else {
                    let scopes = intersect_scopes(&requested_scopes, &refresh_token.scope);
                    if scopes.len() != requested_scopes.len() {
                        return Err(
                            ErrorResponse::new(ErrorCode::InvalidScope).with_description(
                                "requested scope must be a subset of refresh token scope",
                            ),
                        );
                    }
                    scopes
                };

                let principal = self.find_principal(key.id).await?.ok_or_else(|| {
                    ErrorResponse::new(ErrorCode::InvalidGrant)
                        .with_description("principal not found for refresh token")
                })?;

                self.issue_tokens_for_client(
                    &client,
                    principal.as_ref(),
                    &scopes,
                    refresh_token.resource.as_deref(),
                    refresh_token.authorization_details.as_deref(),
                    None,
                    authorizer,
                )
                .await
            }
            TokenRequest::TokenExchange(request) => {
                if request.subject_token_type != SubjectTokenType::AccessToken {
                    return Err(ErrorResponse::new(ErrorCode::InvalidRequest)
                        .with_description("subject_token_type must be access_token"));
                }

                let (jwt_header, _) = decode_jwt::<StandardClaims>(&request.subject_token)?;
                let jwk = self.find_public_jwk(jwt_header.kid).await?;
                let (_, subject_token) =
                    verify_jwt::<StandardClaims>(&jwk, &request.subject_token)?;
                let now = Utc::now().timestamp();
                if subject_token.r#type != TokenType::Bearer
                    || subject_token.r#use != TokenUse::Access
                    || subject_token.iss != self.oauth_config.issuer
                    || subject_token.exp <= now
                    || subject_token.nbf > now
                    || subject_token.aud.is_empty()
                {
                    return Err(ErrorResponse::new(ErrorCode::InvalidGrant)
                        .with_description("invalid subject access token"));
                }

                let key = self
                    .key_service
                    .key_repo()
                    .find_by_id(jwt_header.kid)
                    .await?
                    .ok_or_else(|| {
                        ErrorResponse::new(ErrorCode::InvalidGrant)
                            .with_description("refresh token signing key not found")
                    })?;

                // TODO: get a derevided key from the key ring store to validate token.

                let client = self
                    .client_repo
                    .find_client_by_client_id(&subject_token.client_id)
                    .await
                    .map_err(ErrorResponse::from)?
                    .ok_or_else(|| {
                        ErrorResponse::new(ErrorCode::InvalidClient)
                            .with_description("client not found")
                    })?;
                self.authenticate_client_for_token_endpoint(&client, client_auth.as_ref())?;

                let requested_scopes = request
                    .scope
                    .as_deref()
                    .map(parse_scopes)
                    .unwrap_or_default();
                let scopes = if requested_scopes.is_empty() {
                    subject_token.scope
                } else {
                    intersect_scopes(&requested_scopes, &subject_token.scope)
                };

                if let Some(details) = &request.authorization_details {
                    validate_authorization_details(details)?;
                }

                let principal = self.find_principal(key.id).await?.ok_or_else(|| {
                    ErrorResponse::new(ErrorCode::InvalidGrant)
                        .with_description("principal not found for subject_token")
                })?;
                let resource = request
                    .resource
                    .as_deref()
                    .or(subject_token.resource.as_deref());

                self.issue_tokens_for_client(
                    &client,
                    principal.as_ref(),
                    &scopes,
                    resource,
                    request.authorization_details.as_deref(),
                    resource,
                    authorizer,
                )
                .await
            }
        }
    }

    fn validate_grant_type(
        &self,
        client: &Client,
        grant_type: GrantType,
    ) -> ErrorResponseResult<()> {
        if client.allowed_grant_types.is_empty() || client.allowed_grant_types.contains(&grant_type)
        {
            return Ok(());
        }

        Err(ErrorResponse::new(ErrorCode::UnauthorizedClient)
            .with_description("client is not authorized for this grant type"))
    }

    fn normalize_scopes(scopes: &[String]) -> String {
        let mut normalized = scopes.to_vec();
        normalized.sort();
        normalized.dedup();
        normalized.join(" ")
    }

    fn authenticate_client_for_token_endpoint(
        &self,
        client: &Client,
        client_auth: Option<&OAuth2ClientAuth>,
    ) -> ErrorResponseResult<()> {
        match client.client_type {
            ClientType::Confidential => {
                let auth = client_auth.ok_or_else(|| {
                    ErrorResponse::new(ErrorCode::InvalidClient)
                        .with_description("client authentication is required")
                })?;

                if auth.client_id != client.client_id {
                    return Err(ErrorResponse::new(ErrorCode::InvalidClient)
                        .with_description("client_id does not match token subject"));
                }

                if auth.client_secret.as_deref() != Some(client.client_secret.as_str()) {
                    return Err(ErrorResponse::new(ErrorCode::InvalidClient)
                        .with_description("invalid client credentials"));
                }

                Ok(())
            }
            ClientType::Public => {
                if let Some(auth) = client_auth
                    && auth.client_id != client.client_id
                {
                    return Err(ErrorResponse::new(ErrorCode::InvalidClient)
                        .with_description("client_id does not match token subject"));
                }

                Ok(())
            }
        }
    }

    pub async fn revoke(&self, request: RevocationRequest) -> ErrorResponseResult<()> {
        if request.token.trim().is_empty() {
            return Err(
                ErrorResponse::new(ErrorCode::InvalidRequest).with_description("token is required")
            );
        }

        Err(ErrorResponse::new(ErrorCode::UnsupportedGrantType)
            .with_description("token revocation is not implemented"))
    }

    pub async fn list_jwks(&self) -> ErrorResponseResult<Jwks> {
        let keys = self
            .key_service
            .key_repo()
            .list_active()
            .await
            .map_err(ErrorResponse::from)?;

        let mut jwks = Vec::new();
        for key in keys {
            if let Ok(jwk) = self.load_signing_jwk(&key).await {
                jwks.push(JwkPublic::from(jwk));
            }
        }

        Ok(Jwks { keys: jwks })
    }

    pub async fn list_client_keys(&self, client_id: &str) -> ErrorResponseResult<Vec<Key>> {
        let client = self
            .client_repo
            .find_client_by_client_id(client_id)
            .await
            .map_err(ErrorResponse::from)?
            .ok_or_else(|| {
                ErrorResponse::new(ErrorCode::InvalidClient).with_description("client not found")
            })?;

        let key = self
            .key_service
            .key_repo()
            .find_active_entity_root_key(EntityType::Client, client.id)
            .await
            .map_err(ErrorResponse::from)?;

        if let Some(key) = key {
            Ok(vec![key])
        } else {
            Ok(Vec::new())
        }
    }

    pub async fn find_public_jwk(&self, key_id: u32) -> ErrorResponseResult<JwkPublic> {
        let key = self
            .key_service
            .key_repo()
            .find_by_id(key_id)
            .await
            .map_err(ErrorResponse::from)?
            .ok_or_else(|| {
                ErrorResponse::new(ErrorCode::NotFound).with_description("key not found")
            })?;

        self.ensure_key_is_active_entity_root(&key).await?;

        let private_key = self
            .key_service
            .private_key_repo()
            .load(
                &self
                    .key_service
                    .scoped_namespace(key.entity_type, key.entity_id),
                &key.derivation_path()?,
            )?
            .ok_or_else(|| {
                ErrorResponse::new(ErrorCode::ServerError)
                    .with_description("signing key not found in private key repository")
            })?;

        key.to_jwk_public(&private_key).map_err(ErrorResponse::from)
    }

    pub fn metadata(&self) -> AuthorizationServerMetadata {
        self.oauth_config.to_metadata()
    }

    pub async fn issue_tunnel_authorization(
        &self,
        principal: &dyn Principal,
        application_id: i64,
        request: TunnelAuthorizationRequest,
    ) -> ErrorResponseResult<TunnelAuthorization> {
        if principal.get_entity_type() != EntityType::User {
            return Err(ErrorResponse::new(ErrorCode::AccessDenied));
        }
        let now = Utc::now();
        let claims = TunnelAuthorizationClaims {
            iss: self.oauth_config.issuer.clone(),
            sub: principal.get_entity_id().to_string(),
            application_id,
            vault_id_hash: request.vault_id_hash,
            local_public_key: request.local_public_key,
            remote_public_key: request.remote_public_key,
            exp: (now + TUNNEL_AUTHORIZATION_TTL).timestamp(),
            iat: now.timestamp(),
            nbf: now.timestamp(),
        };
        let signing_jwk = self.load_signing_jwk(principal.get_key()).await?;
        let token = encode_jwt(&signing_jwk, &claims)?;
        Ok(TunnelAuthorization {
            token,
            expires_at: claims.exp,
        })
    }

    async fn load_signing_jwk(&self, key: &Key) -> ErrorResponseResult<JwkPrivate> {
        if let Some(private_key) = self.key_service.private_key_repo().load(
            &self
                .key_service
                .scoped_namespace(key.entity_type, key.entity_id),
            &key.derivation_path()?,
        )? {
            return Ok(key.to_jwk_private(&private_key)?);
        }
        Err(ErrorResponse::new(ErrorCode::ServerError)
            .with_description("signing key not found in private key repository"))
    }

    pub fn device_authorization(
        &self,
        request: DeviceAuthorizationRequest,
    ) -> ErrorResponseResult<DeviceAuthorization> {
        if request.client_id.as_deref().is_some_and(str::is_empty) {
            return Err(ErrorResponse::new(ErrorCode::InvalidRequest)
                .with_description("client_id cannot be empty"));
        }

        let issuer = self.oauth_config.issuer.trim_end_matches('/');
        let user_code = generate_random_string::<32>();

        Ok(DeviceAuthorization {
            device_code: Some(generate_random_string::<32>()),
            expires_in: Some(self.oauth_config.device_code_ttl_secs),
            interval: Some(self.oauth_config.device_poll_interval_secs),
            user_code: Some(user_code.clone()),
            verification_uri: Some(format!("{issuer}/oauth2/device/verify")),
            verification_uri_complete: Some(format!(
                "{issuer}/oauth2/device/verify?user_code={user_code}"
            )),
        })
    }

    async fn issue_tokens_for_client<T>(
        &self,
        client: &Client,
        principal: &dyn Principal,
        scopes: &[String],
        resource: Option<&str>,
        authorization_details: Option<&[AuthorizationDetail]>,
        audience: Option<&str>,
        authorizer: Option<&T>,
    ) -> ErrorResponseResult<TokenResponse>
    where
        T: TokenIssuerAuthorizer,
    {
        let now = Utc::now();
        let signing_jwk = self.load_signing_jwk(principal.get_key()).await?;
        let scope = if scopes.is_empty() {
            None
        } else {
            Some(scopes.join(" "))
        };

        let access_claims = StandardClaims {
            r#type: TokenType::Bearer,
            r#use: TokenUse::Access,
            exp: (now + Duration::seconds(self.oauth_config.token_ttl_secs as i64)).timestamp(),
            iat: now.timestamp(),
            nbf: now.timestamp(),
            iss: self.oauth_config.issuer.clone(),
            aud: audience.unwrap_or(&client.client_id).to_string(),
            client_id: client.client_id.clone(),
            sub: principal.get_entity_id().to_string(),
            scope: scopes.to_vec(),
            resource: resource.map(str::to_string),
            authorization_details: authorization_details
                .map(<[model::contract::AuthorizationDetail]>::to_vec),
        };

        if let (Some(authorizer), Some(details)) = (authorizer, authorization_details) {
            authorizer
                .authorize(client, &access_claims.sub, details)
                .await?;
        }

        let access_token_value = encode_jwt(&signing_jwk, &access_claims)?;

        let user_info = match principal.get_entity_type() {
            EntityType::User => principal
                .get_entity_as_any()
                .downcast_ref::<User>()
                .cloned()
                .map(User::into),
            _ => None,
        };

        let id_token = IdTokenClaims {
            standard_claims: StandardClaims {
                r#use: TokenUse::Id,
                ..access_claims.clone()
            },
            user_info,
        };

        let id_token_value = encode_jwt(&signing_jwk, &id_token)?;

        let refresh_claims = StandardClaims {
            r#use: TokenUse::Refresh,
            exp: (now + Duration::seconds(self.oauth_config.refresh_token_ttl_secs as i64))
                .timestamp(),
            ..access_claims
        };

        let refresh_token_value = encode_jwt(&signing_jwk, &refresh_claims)?;

        Ok(TokenResponse {
            id_token: IdToken(id_token_value),
            access_token: AccessToken(access_token_value),
            token_type: TokenType::Bearer,
            expires_in: Some(self.oauth_config.token_ttl_secs),
            refresh_token_expires_in: Some(self.oauth_config.refresh_token_ttl_secs),
            refresh_token: Some(RefreshToken(refresh_token_value)),
            scope,
            issuer: Some(self.oauth_config.issuer.clone()),
        })
    }

    pub async fn find_user_info(&self, user_id: i64) -> ErrorResponseResult<UserInfo> {
        let user = self
            .user_repo
            .find_user_by_id(user_id)
            .await
            .map_err(ErrorResponse::from)?
            .ok_or_else(|| {
                ErrorResponse::new(ErrorCode::NotFound)
                    .with_description("User not found".to_string())
            })?;

        self.hydrate_user_info(user).await
    }

    pub async fn list_user_info(
        &self,
        offset: u32,
        limit: u32,
    ) -> ErrorResponseResult<Vec<UserInfo>> {
        let users = self
            .user_repo
            .list_users(offset, limit)
            .await
            .map_err(ErrorResponse::from)?;
        let mut user_info_list = Vec::with_capacity(users.len());

        for user in users {
            user_info_list.push(self.hydrate_user_info(user).await?);
        }

        Ok(user_info_list)
    }

    pub async fn update_user_info(
        &self,
        user_id: i64,
        request: UpdateUserInfoRequest,
    ) -> ErrorResponseResult<UserInfo> {
        let existing = self
            .user_repo
            .find_user_by_id(user_id)
            .await
            .map_err(ErrorResponse::from)?
            .ok_or_else(|| {
                ErrorResponse::new(ErrorCode::NotFound)
                    .with_description("User not found".to_string())
            })?;

        let sex = match request.sex.as_deref() {
            Some("male") => Some(idp_model::contract::Sex::Male),
            Some("female") => Some(idp_model::contract::Sex::Female),
            Some(value) => {
                return Err(ErrorResponse::new(ErrorCode::InvalidRequest)
                    .with_description(format!("unsupported sex value: {value}")));
            }
            None => existing.sex,
        };

        let birthdate = match request.birthdate.as_deref() {
            Some(value) => Some(
                chrono::DateTime::parse_from_rfc3339(value)
                    .map_err(|_| {
                        ErrorResponse::new(ErrorCode::InvalidRequest)
                            .with_description("birthdate must be RFC3339".to_string())
                    })?
                    .with_timezone(&Utc),
            ),
            None => existing.birthdate,
        };

        let user = User {
            id: existing.id,
            name: request.name.unwrap_or(existing.name),
            given_name: request.given_name.or(existing.given_name),
            family_name: request.family_name.or(existing.family_name),
            middle_name: request.middle_name.or(existing.middle_name),
            nickname: request.nickname.or(existing.nickname),
            profile: request.profile.or(existing.profile),
            picture: request.picture.or(existing.picture),
            website: request.website.or(existing.website),
            sex,
            birthdate,
            zoneinfo: request.zoneinfo.or(existing.zoneinfo),
            locale: request.locale.or(existing.locale),
            created_at: existing.created_at,
            updated_at: Utc::now(),
        };

        let user = self
            .user_repo
            .update_user(user)
            .await
            .map_err(ErrorResponse::from)?;

        if let Some(email) = request.email {
            self.user_repo
                .upsert_primary_user_email(user.id, &email, request.email_verified.unwrap_or(false))
                .await
                .map_err(ErrorResponse::from)?;
        }

        if let Some(phone_number) = request.phone_number {
            self.user_repo
                .upsert_primary_user_phone_number(
                    user.id,
                    &phone_number,
                    request.phone_number_verified.unwrap_or(false),
                )
                .await
                .map_err(ErrorResponse::from)?;
        }

        self.hydrate_user_info(user).await
    }

    pub async fn reset_user_password(
        &self,
        user_id: i64,
        password: &str,
    ) -> ErrorResponseResult<()> {
        if self
            .user_repo
            .find_user_by_id(user_id)
            .await
            .map_err(ErrorResponse::from)?
            .is_none()
        {
            return Err(ErrorResponse::new(ErrorCode::NotFound)
                .with_description("User not found".to_string()));
        }

        self.user_repo
            .replace_user_password(user_id, password)
            .await
            .map_err(ErrorResponse::from)
    }

    pub async fn delete_user(&self, user_id: i64) -> ErrorResponseResult<()> {
        if self
            .user_repo
            .find_user_by_id(user_id)
            .await
            .map_err(ErrorResponse::from)?
            .is_none()
        {
            return Err(ErrorResponse::new(ErrorCode::NotFound)
                .with_description("User not found".to_string()));
        }

        self.user_repo
            .delete_user_by_id(user_id)
            .await
            .map_err(ErrorResponse::from)
    }

    pub async fn list_user_consents(
        &self,
        user_id: i64,
        offset: u32,
        limit: u32,
    ) -> ErrorResponseResult<Vec<idp_model::model::OAuth2UserConsent>> {
        if self
            .user_repo
            .find_user_by_id(user_id)
            .await
            .map_err(ErrorResponse::from)?
            .is_none()
        {
            return Err(ErrorResponse::new(ErrorCode::NotFound)
                .with_description("User not found".to_string()));
        }

        self.oauth2_user_consent_repo
            .list_user_consents(user_id, offset, limit)
            .await
            .map_err(ErrorResponse::from)
    }

    pub async fn revoke_user_consent(
        &self,
        user_id: i64,
        consent_id: i64,
    ) -> ErrorResponseResult<()> {
        if self
            .user_repo
            .find_user_by_id(user_id)
            .await
            .map_err(ErrorResponse::from)?
            .is_none()
        {
            return Err(ErrorResponse::new(ErrorCode::NotFound)
                .with_description("User not found".to_string()));
        }

        let consent = self
            .oauth2_user_consent_repo
            .find_user_consent_by_id(consent_id)
            .await
            .map_err(ErrorResponse::from)?
            .ok_or_else(|| {
                ErrorResponse::new(ErrorCode::NotFound)
                    .with_description("User consent not found".to_string())
            })?;

        if consent.user_id != user_id {
            return Err(ErrorResponse::new(ErrorCode::NotFound)
                .with_description("User consent not found".to_string()));
        }

        self.oauth2_user_consent_repo
            .delete_user_consent_by_id(consent_id)
            .await
            .map_err(ErrorResponse::from)
    }

    async fn hydrate_user_info(&self, user: User) -> ErrorResponseResult<UserInfo> {
        let user_id = user.id;
        let mut user_info: UserInfo = From::from(user);

        let emails = self
            .user_repo
            .find_user_emails_by_user_id(user_id)
            .await
            .map_err(ErrorResponse::from)?;
        if let Some(primary_email) = emails.into_iter().find(|e| e.primary) {
            user_info.email = Some(primary_email.email);
            user_info.email_verified = Some(primary_email.verified);
        }

        let phone_numbers = self
            .user_repo
            .find_user_phone_numbers_by_user_id(user_id)
            .await
            .map_err(ErrorResponse::from)?;
        if let Some(primary_phone_number) = phone_numbers.into_iter().find(|e| e.primary) {
            user_info.phone_number = Some(primary_phone_number.phone_number);
            user_info.phone_number_verified = Some(primary_phone_number.verified);
        }

        Ok(user_info)
    }

    pub async fn find_principal(
        &self,
        key_id: u32,
    ) -> ErrorResponseResult<Option<Box<dyn Principal>>> {
        let key = if let Some(key) = self.key_service.key_repo().find_by_id(key_id).await? {
            key
        } else {
            return Ok(None);
        };

        if self.ensure_key_is_active_entity_root(&key).await.is_err() {
            return Ok(None);
        }

        let principal =
            match key.entity_type {
                EntityType::User => {
                    let user =
                        if let Some(user) = self.user_repo.find_user_by_id(key.entity_id).await? {
                            user
                        } else {
                            return Ok(None);
                        };

                    Box::new(UserPrincipal { user, key }) as Box<dyn Principal>
                }
                _ => {
                    return Err(ErrorResponse::new(ErrorCode::ServerError).with_description(
                        format!("unsupported principal entity type: {}", key.entity_type),
                    ));
                }
            };

        Ok(Some(principal))
    }

    async fn ensure_key_is_active_entity_root(&self, key: &Key) -> ErrorResponseResult<()> {
        let active_root = self
            .key_service
            .key_repo()
            .find_active_entity_root_key(key.entity_type, key.entity_id)
            .await
            .map_err(ErrorResponse::from)?;

        let active_root = active_root.ok_or_else(|| {
            ErrorResponse::new(ErrorCode::InvalidGrant)
                .with_description("active entity root key not found")
        })?;

        if active_root.id != key.id {
            return Err(ErrorResponse::new(ErrorCode::InvalidGrant)
                .with_description("key is not the active entity root"));
        }

        Ok(())
    }
}
