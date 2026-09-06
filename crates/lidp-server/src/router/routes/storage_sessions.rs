use axum::{Json, extract::State};
use lidp_model::contract::{ErrorCode, ErrorResponse, StorageSession};
use lidp_service::storage_session::StorageScope;

use crate::router::{RouterState, middleware::StandardAuthorization};

#[utoipa::path(
    post,
    path = "/storage/sessions",
    responses((status = 200, description = "Storage session", body = StorageSession)),
    security(("authorization" = []))
)]
pub(crate) async fn create_storage_session(
    State(state): State<RouterState>,
    StandardAuthorization { claims, .. }: StandardAuthorization,
) -> Result<Json<StorageSession>, ErrorResponse> {
    state
        .storage_sessions
        .issue(StorageScope {
            user_sub: claims.sub,
            client_id: claims.aud,
        })
        .map(Json)
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))
}
