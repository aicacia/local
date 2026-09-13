use axum::{
    Json,
    extract::{Form, State},
};
use idp_model::contract::{
    DeviceAuthorization, DeviceAuthorizationRequest, ErrorCode, ErrorResponse,
};

use crate::router::{RouterState, middleware::require_current_global_identity};

#[utoipa::path(
    post,
    path = "/oauth2/device/auth",
    request_body(content = DeviceAuthorizationRequest, content_type = "application/x-www-form-urlencoded"),
    responses((status = 200, description = "Device authorization", body = DeviceAuthorization))
)]
pub(crate) async fn device_auth(
    State(state): State<RouterState>,
    Form(request): Form<DeviceAuthorizationRequest>,
) -> Result<Json<DeviceAuthorization>, ErrorResponse> {
    require_current_global_identity(&state).await?;
    let response = state.oauth2_service.device_authorization(request)?;
    Ok(Json(response))
}

#[utoipa::path(get, path = "/oauth2/device/verify", responses((status = 302, description = "Device verify redirect")))]
pub(crate) async fn device_verify(
    State(_state): State<RouterState>,
) -> Result<Json<DeviceAuthorization>, ErrorResponse> {
    Err(ErrorResponse::new(ErrorCode::NotImplemented)
        .with_description("endpoint is not implemented"))
}
