use axum::{
    Json,
    extract::{Path, State},
};
use lidp_model::contract::{
    DeviceApprovalRequest, DeviceEnrollment, DeviceEnrollmentRequest, DeviceInfo, ErrorCode,
    ErrorResponse, TrustedDevice, UpdateDeviceRequest,
};
use lidp_service::device_enrollment::DeviceEnrollmentService;
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

#[utoipa::path(
    post,
    path = "/devices/enrollments",
    request_body = DeviceEnrollmentRequest,
    responses((status = 200, description = "Device enrollment", body = DeviceEnrollment)),
    security(("authorization" = []))
)]
pub(crate) async fn enroll_device(
    State(state): State<RouterState>,
    StandardAuthorization { principal, .. }: StandardAuthorization,
    Json(request): Json<DeviceEnrollmentRequest>,
) -> Result<Json<DeviceEnrollment>, ErrorResponse> {
    DeviceEnrollmentService::new(state.user_devices)
        .enroll(principal.get_entity_id(), request)
        .await
        .map(Json)
}

#[utoipa::path(
    post,
    path = "/devices/enrollments/{id}/approve",
    params(("id" = i64, Path, description = "Device enrollment ID")),
    request_body = DeviceApprovalRequest,
    responses((status = 200, description = "Approved device", body = DeviceInfo)),
    security(("authorization" = []))
)]
pub(crate) async fn approve_device(
    State(state): State<RouterState>,
    Path(device_id): Path<i64>,
    StandardAuthorization { principal, .. }: StandardAuthorization,
    Json(request): Json<DeviceApprovalRequest>,
) -> Result<Json<DeviceInfo>, ErrorResponse> {
    DeviceEnrollmentService::new(state.user_devices)
        .approve(principal.get_entity_id(), device_id, &request.approval_code)
        .await
        .map(Json)
}

#[utoipa::path(
    get,
    path = "/devices",
    responses((status = 200, description = "User devices", body = [DeviceInfo])),
    security(("authorization" = []))
)]
pub(crate) async fn list_devices(
    State(state): State<RouterState>,
    StandardAuthorization { principal, .. }: StandardAuthorization,
) -> Result<Json<Vec<DeviceInfo>>, ErrorResponse> {
    DeviceEnrollmentService::new(state.user_devices)
        .list(principal.get_entity_id())
        .await
        .map(Json)
}

#[utoipa::path(
    patch,
    path = "/devices/{id}",
    params(("id" = i64, Path, description = "Device ID")),
    request_body = UpdateDeviceRequest,
    responses((status = 200, description = "Updated device", body = DeviceInfo)),
    security(("authorization" = []))
)]
pub(crate) async fn update_device(
    State(state): State<RouterState>,
    Path(device_id): Path<i64>,
    StandardAuthorization { principal, .. }: StandardAuthorization,
    Json(request): Json<UpdateDeviceRequest>,
) -> Result<Json<DeviceInfo>, ErrorResponse> {
    DeviceEnrollmentService::new(state.user_devices)
        .rename(principal.get_entity_id(), device_id, request)
        .await
        .map(Json)
}

#[utoipa::path(
    delete,
    path = "/devices/{id}",
    params(("id" = i64, Path, description = "Device ID")),
    responses((status = 204, description = "Revoked device")),
    security(("authorization" = []))
)]
pub(crate) async fn revoke_device(
    State(state): State<RouterState>,
    Path(device_id): Path<i64>,
    StandardAuthorization { principal, .. }: StandardAuthorization,
) -> Result<(), ErrorResponse> {
    DeviceEnrollmentService::new(state.user_devices)
        .revoke(principal.get_entity_id(), device_id)
        .await
}
