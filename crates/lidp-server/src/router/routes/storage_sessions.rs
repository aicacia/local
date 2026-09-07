use axum::{Json, extract::State};
use http::{HeaderMap, header::AUTHORIZATION};
use lidp_model::contract::{ErrorCode, ErrorResponse, StorageSession};
use lidp_service::{repo::UserDeviceRepo, storage_session::StorageScope};

use crate::router::{RouterState, authorize_bearer};

#[utoipa::path(
    post,
    path = "/storage/sessions",
    responses((status = 200, description = "Storage session", body = StorageSession)),
    security(("authorization" = []))
)]
pub async fn create_storage_session(
    State(state): State<RouterState>,
    headers: HeaderMap,
) -> Result<Json<StorageSession>, ErrorResponse> {
    let token = bearer_token(&headers)?;
    let scope = match &state.storage_scope_resolver {
        Some(resolver) => resolver.resolve(token.to_owned()).await?,
        None => local_scope(&state, token).await?,
    };
    state
        .storage_sessions
        .issue(scope)
        .map(Json)
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))
}

async fn local_scope(state: &RouterState, token: &str) -> Result<StorageScope, ErrorResponse> {
    let authorization = authorize_bearer(state, token).await?;
    if !authorization
        .claims
        .scope
        .iter()
        .any(|scope| scope == "storage")
    {
        return Err(ErrorResponse::new(ErrorCode::AccessDenied));
    }
    let application_id = state
        .oauth2_service
        .application_id_for_client(&authorization.claims.aud)
        .await?;
    let trusted_devices = state
        .user_devices
        .list_approved_by_user_id(authorization.principal.get_entity_id())
        .await
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?;
    Ok(StorageScope {
        user_sub: authorization.claims.sub,
        application_id,
        principal_key_id: authorization.principal.get_key().id,
        trusted_devices,
        access_token: authorization.token,
    })
}

fn bearer_token(headers: &HeaderMap) -> Result<&str, ErrorResponse> {
    headers
        .get(AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .filter(|token| !token.is_empty())
        .ok_or_else(|| ErrorResponse::new(ErrorCode::NotAuthorized))
}
