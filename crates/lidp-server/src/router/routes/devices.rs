use axum::{Json, extract::State};
use lidp_model::contract::{ErrorCode, ErrorResponse, TrustedDevice};
use lidp_service::repo::UserDeviceRepo;

use crate::router::{RouterState, middleware::StandardAuthorization};

#[utoipa::path(
    get,
    path = "/devices/trusted",
    responses((status = 200, description = "Approved user devices", body = [TrustedDevice])),
    security(("authorization" = []))
)]
pub(crate) async fn trusted_devices(
    State(state): State<RouterState>,
    StandardAuthorization { principal, .. }: StandardAuthorization,
) -> Result<Json<Vec<TrustedDevice>>, ErrorResponse> {
    let devices = state
        .user_devices
        .list_approved_by_user_id(principal.get_entity_id())
        .await
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?;
    Ok(Json(devices))
}
