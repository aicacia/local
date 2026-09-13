use axum::{Json, extract::State};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use idp_model::contract::{
    DeviceSelfRevocationRequest, ErrorCode, ErrorResponse, device_self_revocation_payload,
};
use iroh::{EndpointId, Signature};
use management_service::DeviceEnrollmentService;

use crate::RouterState;

#[utoipa::path(
    post,
    path = "/devices/revoke-self",
    request_body = DeviceSelfRevocationRequest,
    responses((status = 200, description = "Device revoked"))
)]
pub(crate) async fn revoke_self(
    State(state): State<RouterState>,
    Json(request): Json<DeviceSelfRevocationRequest>,
) -> Result<(), ErrorResponse> {
    if let Some(control_plane) = &state.hosted_control_plane {
        return control_plane
            .revoke_self(request)
            .await
            .map_err(|_| ErrorResponse::new(ErrorCode::ServerError));
    }

    verify_request(&request)?;
    DeviceEnrollmentService::new(state.devices)
        .revoke_self(&request.public_key)
        .await
}

fn verify_request(request: &DeviceSelfRevocationRequest) -> Result<(), ErrorResponse> {
    let public_key = request
        .public_key
        .parse::<EndpointId>()
        .map_err(|_| ErrorResponse::new(ErrorCode::AccessDenied))?;
    let signature = URL_SAFE_NO_PAD
        .decode(&request.signature)
        .map_err(|_| ErrorResponse::new(ErrorCode::AccessDenied))?;
    let signature = Signature::try_from(signature.as_slice())
        .map_err(|_| ErrorResponse::new(ErrorCode::AccessDenied))?;
    public_key
        .verify(
            device_self_revocation_payload(&request.public_key).as_bytes(),
            &signature,
        )
        .map_err(|_| ErrorResponse::new(ErrorCode::AccessDenied))
}
