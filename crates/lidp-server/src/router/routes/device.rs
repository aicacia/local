use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::RouterState;

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Device {
    public_key: String,
    address: String,
}

#[derive(Deserialize, ToSchema)]
pub(crate) struct SignDeviceMessage {
    message: String,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct DeviceSignature {
    signature: String,
}

#[utoipa::path(
    get,
    path = "/device",
    responses((status = 200, description = "Local device identity", body = Device))
)]
pub(crate) async fn device(State(state): State<RouterState>) -> Result<Json<Device>, StatusCode> {
    Ok(Json(Device {
        public_key: state.device_identity.endpoint_id().to_string(),
        address: state
            .device_identity
            .endpoint_address()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
    }))
}

#[utoipa::path(
    post,
    path = "/device/sign",
    request_body = SignDeviceMessage,
    responses((status = 200, description = "Device message signature", body = DeviceSignature))
)]
pub(crate) async fn sign_device_message(
    State(state): State<RouterState>,
    Json(request): Json<SignDeviceMessage>,
) -> Json<DeviceSignature> {
    Json(DeviceSignature {
        signature: state.device_identity.sign(request.message.as_bytes()),
    })
}
