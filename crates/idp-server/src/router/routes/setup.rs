use axum::{Json, extract::State};
use idp_model::contract::{
    DeviceState, ErrorCode, ErrorResponse, SetupBootstrapRegistration, SetupBootstrapRequest,
};
use iroh::{EndpointAddr, EndpointId};
use sha2::{Digest, Sha256};

use management_service::DeviceRepo;

use crate::{RouterState, router::middleware::StandardAuthorization};

#[utoipa::path(
    post,
    path = "/setup/bootstrap",
    request_body = SetupBootstrapRequest,
    responses((status = 200, description = "Bootstrap grant", body = SetupBootstrapRegistration)),
    security(("authorization" = []))
)]
pub(crate) async fn register_bootstrap(
    State(state): State<RouterState>,
    StandardAuthorization { claims, .. }: StandardAuthorization,
    Json(request): Json<SetupBootstrapRequest>,
) -> Result<Json<SetupBootstrapRegistration>, ErrorResponse> {
    if !claims.scope.iter().any(|scope| scope == "setup") {
        return Err(ErrorResponse::new(ErrorCode::AccessDenied));
    }
    let endpoint_id = request
        .endpoint_id
        .parse::<EndpointId>()
        .map_err(|_| ErrorResponse::new(ErrorCode::InvalidRequest))?;
    let endpoint_addr = serde_json::from_str::<EndpointAddr>(&request.endpoint_addr)
        .map_err(|_| ErrorResponse::new(ErrorCode::InvalidRequest))?;
    if endpoint_id.to_string() != request.endpoint_id
        || endpoint_addr.id != endpoint_id
        || request.device_name.trim().is_empty()
    {
        return Err(ErrorResponse::new(ErrorCode::InvalidRequest));
    }
    let grant = idp_service::generate_random_string::<32>();
    let existing_devices = state
        .devices
        .list()
        .await
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?;
    let existing = existing_devices
        .iter()
        .find(|device| device.public_key == endpoint_id.to_string());
    if existing.is_some_and(|device| device.owner_subject != claims.sub) {
        return Err(ErrorResponse::new(ErrorCode::AccessDenied));
    }
    let (device_id, enrollment_code_hash) = match existing {
        Some(device) if device.state == DeviceState::Approved => (device.id, Vec::new()),
        Some(device) if device.state == DeviceState::Pending => {
            state
                .devices
                .revoke(
                    &claims.sub,
                    device.id,
                    &state.device_identity.endpoint_id().to_string(),
                )
                .await
                .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?;
            create_pending_device(&state, &claims.sub, request, endpoint_id, &grant).await?
        }
        _ => create_pending_device(&state, &claims.sub, request, endpoint_id, &grant).await?,
    };
    let (grant, record) =
        state
            .bootstrap_grants
            .register(grant, endpoint_id, device_id, enrollment_code_hash);
    let expires_at = record.expires_at_unix();
    Ok(Json(SetupBootstrapRegistration {
        grant,
        endpoint_id: state.device_identity.endpoint_id().to_string(),
        endpoint_addr: state
            .device_identity
            .endpoint_address()
            .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?,
        expires_at,
    }))
}

async fn create_pending_device(
    state: &RouterState,
    owner_subject: &str,
    request: SetupBootstrapRequest,
    endpoint_id: EndpointId,
    grant: &str,
) -> Result<(idp_model::model::Id, Vec<u8>), ErrorResponse> {
    let enrollment_code_hash = Sha256::digest(grant.as_bytes()).to_vec();
    let expiry = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |time| time.as_secs() as i64)
        + 15 * 60;
    let device = state
        .devices
        .create(
            owner_subject.to_owned(),
            request.device_name,
            endpoint_id.to_string(),
            request.endpoint_addr,
            enrollment_code_hash.clone(),
            expiry,
        )
        .await
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?;
    Ok((device.id, enrollment_code_hash))
}
