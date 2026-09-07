use axum::{Json, extract::State};
use lidp_model::contract::{ErrorCode, ErrorResponse, StorageSession};
use lidp_service::{repo::UserDeviceRepo, storage_session::StorageScope};

use crate::router::{RouterState, middleware::StandardAuthorization};

#[utoipa::path(
    post,
    path = "/storage/sessions",
    responses((status = 200, description = "Storage session", body = StorageSession)),
    security(("authorization" = []))
)]
pub(crate) async fn create_storage_session(
    State(state): State<RouterState>,
    StandardAuthorization {
        claims,
        principal,
        token,
        ..
    }: StandardAuthorization,
) -> Result<Json<StorageSession>, ErrorResponse> {
    if !claims.scope.iter().any(|scope| scope == "storage") {
        return Err(ErrorResponse::new(ErrorCode::AccessDenied));
    }
    let application_id = state
        .oauth2_service
        .application_id_for_client(&claims.aud)
        .await?;
    let trusted_devices = state
        .user_devices
        .list_approved_by_user_id(principal.get_entity_id())
        .await
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?;
    state
        .storage_sessions
        .issue(StorageScope {
            user_sub: claims.sub,
            application_id,
            principal_key_id: principal.get_key().id,
            trusted_devices,
            access_token: token,
        })
        .map(Json)
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))
}
