use axum::{Json, extract::State};
use lidp_model::contract::{
    ErrorCode, ErrorResponse, TunnelAuthorization, TunnelAuthorizationRequest,
};
use lidp_service::repo::UserDeviceRepo;

use crate::router::{RouterState, middleware::StandardAuthorization};

#[utoipa::path(
    post,
    path = "/devices/tunnels",
    request_body = TunnelAuthorizationRequest,
    responses((status = 200, description = "Tunnel authorization", body = TunnelAuthorization)),
    security(("authorization" = []))
)]
pub(crate) async fn create_tunnel_authorization(
    State(state): State<RouterState>,
    StandardAuthorization {
        claims, principal, ..
    }: StandardAuthorization,
    Json(request): Json<TunnelAuthorizationRequest>,
) -> Result<Json<TunnelAuthorization>, ErrorResponse> {
    if request.vault_id_hash.is_empty()
        || request.local_public_key.is_empty()
        || request.remote_public_key.is_empty()
        || request.local_public_key == request.remote_public_key
    {
        return Err(ErrorResponse::new(ErrorCode::InvalidRequest));
    }
    let approved = state
        .user_devices
        .are_approved_by_user_id(
            principal.get_entity_id(),
            &request.local_public_key,
            &request.remote_public_key,
        )
        .await
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?;
    if !approved {
        return Err(ErrorResponse::new(ErrorCode::AccessDenied));
    }
    state
        .oauth2_service
        .issue_tunnel_authorization(principal.as_ref(), claims.aud, request)
        .await
        .map(Json)
}
