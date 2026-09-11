use axum::{
    Json,
    extract::{Path, State},
};
use idp_model::contract::{
    DeviceEnrollment, DeviceEnrollmentRequest, DeviceInfo, ErrorCode, ErrorResponse,
    PairingAcceptance, TrustedDevice, UpdateDeviceRequest,
};
use management_service::{DeviceEnrollmentService, DeviceRepo};

use crate::router::{PairingAcceptanceController, RouterState, middleware::StandardAuthorization};

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
    get,
    path = "/devices/pairing-accepting",
    responses((status = 200, description = "Pairing acceptance state", body = PairingAcceptance)),
    security(("authorization" = []))
)]
pub(crate) async fn pairing_acceptance(
    State(state): State<RouterState>,
    StandardAuthorization { .. }: StandardAuthorization,
) -> Result<Json<PairingAcceptance>, ErrorResponse> {
    state
        .pairing_acceptance
        .pairing_accepting()
        .map(|accepting| Json(PairingAcceptance { accepting }))
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))
}

#[utoipa::path(
    put,
    path = "/devices/pairing-accepting",
    request_body = PairingAcceptance,
    responses((status = 200, description = "Pairing acceptance state", body = PairingAcceptance)),
    security(("authorization" = []))
)]
pub(crate) async fn set_pairing_acceptance(
    State(state): State<RouterState>,
    StandardAuthorization { .. }: StandardAuthorization,
    Json(PairingAcceptance { accepting }): Json<PairingAcceptance>,
) -> Result<Json<PairingAcceptance>, ErrorResponse> {
    state
        .pairing_acceptance
        .set_pairing_accepting(accepting)
        .map(|()| Json(PairingAcceptance { accepting }))
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))
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
