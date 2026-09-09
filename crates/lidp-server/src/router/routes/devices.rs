use axum::{
    Json,
    extract::{Path, State},
};
use lidp_model::contract::{
    DeviceEnrollment, DeviceEnrollmentRequest, DeviceInfo, DevicePairingApprovalPayload,
    DevicePairingApprovalRequest, DevicePairingInvitation, DevicePairingInvitationRequest,
    DevicePairingRedemptionRequest, ErrorCode, ErrorResponse, TrustedDevice, UpdateDeviceRequest,
};
use lidp_service::device_enrollment::DeviceEnrollmentService;
use lidp_service::repo::DeviceRepo;

use crate::router::{RouterState, middleware::StandardAuthorization};

#[utoipa::path(
    get,
    path = "/devices/trusted",
    responses((status = 200, description = "Approved devices", body = [TrustedDevice])),
    security(("authorization" = []))
)]
pub(crate) async fn trusted_devices(
    State(state): State<RouterState>,
    StandardAuthorization { claims, .. }: StandardAuthorization,
) -> Result<Json<Vec<TrustedDevice>>, ErrorResponse> {
    if !claims.scope.iter().any(|scope| scope == "storage") {
        return Err(ErrorResponse::new(ErrorCode::AccessDenied));
    }
    let devices = state
        .devices
        .list_approved()
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
    StandardAuthorization { .. }: StandardAuthorization,
    Json(request): Json<DeviceEnrollmentRequest>,
) -> Result<Json<DeviceEnrollment>, ErrorResponse> {
    DeviceEnrollmentService::new(state.devices)
        .enroll(request)
        .await
        .map(Json)
}

#[utoipa::path(
    post,
    path = "/devices/pairing-invitations",
    request_body = DevicePairingInvitationRequest,
    responses((status = 200, description = "Single-use pairing invitation", body = DevicePairingInvitation)),
    security(("authorization" = []))
)]
pub(crate) async fn create_pairing_invitation(
    State(state): State<RouterState>,
    StandardAuthorization { .. }: StandardAuthorization,
    Json(request): Json<DevicePairingInvitationRequest>,
) -> Result<Json<DevicePairingInvitation>, ErrorResponse> {
    DeviceEnrollmentService::new(state.devices)
        .create_pairing_invitation(request)
        .await
        .map(Json)
}

#[utoipa::path(
    post,
    path = "/devices/pairing-invitations/redeem",
    request_body = DevicePairingRedemptionRequest,
    responses((status = 200, description = "Pending device enrollment", body = DeviceEnrollment))
)]
pub(crate) async fn redeem_pairing_invitation(
    State(state): State<RouterState>,
    Json(request): Json<DevicePairingRedemptionRequest>,
) -> Result<Json<DeviceEnrollment>, ErrorResponse> {
    DeviceEnrollmentService::new(state.devices)
        .redeem_pairing_invitation(request)
        .await
        .map(Json)
}

#[utoipa::path(
    get,
    path = "/devices/enrollments/{id}/approval-payload",
    params(("id" = i64, Path, description = "Pending device enrollment ID")),
    responses((status = 200, description = "Canonical approval payload", body = DevicePairingApprovalPayload)),
    security(("authorization" = []))
)]
pub(crate) async fn pairing_approval_payload(
    State(state): State<RouterState>,
    Path(device_id): Path<i64>,
    StandardAuthorization { .. }: StandardAuthorization,
) -> Result<Json<DevicePairingApprovalPayload>, ErrorResponse> {
    DeviceEnrollmentService::new(state.devices)
        .pairing_approval_payload(device_id)
        .await
        .map(|payload| Json(DevicePairingApprovalPayload { payload }))
}

#[utoipa::path(
    post,
    path = "/devices/enrollments/{id}/approve",
    params(("id" = i64, Path, description = "Pending device enrollment ID")),
    request_body = DevicePairingApprovalRequest,
    responses((status = 200, description = "Approved device", body = DeviceInfo)),
    security(("authorization" = []))
)]
pub(crate) async fn approve_device(
    State(state): State<RouterState>,
    Path(device_id): Path<i64>,
    StandardAuthorization { .. }: StandardAuthorization,
    Json(request): Json<DevicePairingApprovalRequest>,
) -> Result<Json<DeviceInfo>, ErrorResponse> {
    DeviceEnrollmentService::new(state.devices)
        .approve_pairing(device_id, request)
        .await
        .map(Json)
}

#[utoipa::path(
    get,
    path = "/devices",
    responses((status = 200, description = "Devices", body = [DeviceInfo])),
    security(("authorization" = []))
)]
pub(crate) async fn list_devices(
    State(state): State<RouterState>,
    StandardAuthorization { .. }: StandardAuthorization,
) -> Result<Json<Vec<DeviceInfo>>, ErrorResponse> {
    DeviceEnrollmentService::new(state.devices)
        .list()
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
    StandardAuthorization { .. }: StandardAuthorization,
    Json(request): Json<UpdateDeviceRequest>,
) -> Result<Json<DeviceInfo>, ErrorResponse> {
    DeviceEnrollmentService::new(state.devices)
        .rename(device_id, request)
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
    StandardAuthorization { .. }: StandardAuthorization,
) -> Result<(), ErrorResponse> {
    DeviceEnrollmentService::new(state.devices)
        .revoke(device_id, &state.device_identity.endpoint_id().to_string())
        .await
}
