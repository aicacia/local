use std::{future::Future, sync::Arc};

use axum::{
    Json,
    extract::{Form, State},
    http::HeaderMap,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use idp_model::{
    contract::{ErrorCode, ErrorResponse, ErrorResponseResult, OAuth2ClientAuth, TokenRequest},
    model::Client,
};
use idp_service::oauth2::TokenIssuerAuthorizer;
use iroh::EndpointId;
use model::contract::{AuthorizationDetail, StorageAuthorizationAction, TokenResponse};
use storage_service::{Access, ScopedFileSystemRuntime};

use crate::router::{RouterState, middleware::require_current_global_identity};

#[utoipa::path(post, path = "/oauth2/token", request_body(content = TokenRequest, content_type = "application/x-www-form-urlencoded"), responses((status = 200, description = "Token response", body = TokenResponse)))]
pub(crate) async fn token(
    headers: HeaderMap,
    State(state): State<RouterState>,
    Form(request): Form<TokenRequest>,
) -> Result<Json<TokenResponse>, ErrorResponse> {
    require_current_global_identity(&state).await?;
    let client_auth = parse_basic_client_auth(&headers)?;
    let authorizer = state
        .storage_file_systems
        .as_ref()
        .map(|file_systems| StorageTokenAuthorizer::new(Arc::clone(file_systems)));
    let response = state
        .oauth2_service
        .token_with_authorizer(request, client_auth, authorizer.as_ref())
        .await?;
    Ok(Json(response))
}

struct StorageTokenAuthorizer {
    file_systems: Arc<ScopedFileSystemRuntime<EndpointId>>,
}

impl StorageTokenAuthorizer {
    fn new(file_systems: Arc<ScopedFileSystemRuntime<EndpointId>>) -> Self {
        Self { file_systems }
    }
}

impl TokenIssuerAuthorizer for StorageTokenAuthorizer {
    fn authorize<'a>(
        &'a self,
        client: &'a Client,
        subject: &'a str,
        authorization_details: &'a [AuthorizationDetail],
    ) -> impl Future<Output = ErrorResponseResult<()>> + Send + 'a {
        async move {
            let scope = StorageNamespace {
                user_sub: subject,
                application_id: client.application_id,
            };
            let store = self.file_systems.authorization(&scope).await.map_err(|_| {
                ErrorResponse::new(ErrorCode::ServerError)
                    .with_description("storage authorization is unavailable")
            })?;
            for detail in authorization_details {
                let AuthorizationDetail::Storage(detail) = detail;
                for action in &detail.actions {
                    let access = match action {
                        StorageAuthorizationAction::Read => Access::Read,
                        StorageAuthorizationAction::Write => Access::ReadWrite,
                    };
                    if !store.authorize(subject, &detail.folder, access).await {
                        return Err(ErrorResponse::new(ErrorCode::NotAuthorized)
                            .with_description("storage authorization denied"));
                    }
                }
            }
            Ok(())
        }
    }
}

struct StorageNamespace<'a> {
    user_sub: &'a str,
    application_id: i64,
}

impl storage_model::StorageNamespace for StorageNamespace<'_> {
    fn user_sub(&self) -> &str {
        self.user_sub
    }

    fn application_id(&self) -> i64 {
        self.application_id
    }
}

fn parse_basic_client_auth(headers: &HeaderMap) -> Result<Option<OAuth2ClientAuth>, ErrorResponse> {
    let Some(value) = headers.get(axum::http::header::AUTHORIZATION) else {
        return Ok(None);
    };

    let raw = value.to_str().map_err(|_| {
        ErrorResponse::new(idp_model::contract::ErrorCode::InvalidClient)
            .with_description("invalid authorization header encoding")
    })?;

    let Some(encoded) = raw.strip_prefix("Basic ") else {
        return Err(
            ErrorResponse::new(idp_model::contract::ErrorCode::InvalidClient)
                .with_description("unsupported token endpoint authorization method"),
        );
    };

    let decoded = STANDARD.decode(encoded).map_err(|_| {
        ErrorResponse::new(idp_model::contract::ErrorCode::InvalidClient)
            .with_description("invalid basic authorization token")
    })?;

    let decoded = String::from_utf8(decoded).map_err(|_| {
        ErrorResponse::new(idp_model::contract::ErrorCode::InvalidClient)
            .with_description("invalid basic authorization token")
    })?;

    let mut parts = decoded.splitn(2, ':');
    let client_id = parts.next().unwrap_or_default().trim();
    let client_secret = parts.next();

    if client_id.is_empty() {
        return Err(
            ErrorResponse::new(idp_model::contract::ErrorCode::InvalidClient)
                .with_description("client_id is required in basic authorization"),
        );
    }

    Ok(Some(OAuth2ClientAuth {
        client_id: client_id.to_string(),
        client_secret: client_secret.map(str::to_string),
    }))
}
