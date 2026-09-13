use std::sync::Arc;

use axum::{Json, extract::State, http::HeaderMap};
use bootstrap_service::bootstrap::BootstrapInput;
use idp_model::contract::{
    ErrorCode, ErrorResponse, SetupDeviceRequest, SetupDeviceStatus, SetupJoinRequest,
    SetupJoinState, SetupJoinStatus, SetupNewRequest, SetupResidency, SetupStage, SetupStatus,
};
use idp_service::generate_random_string;
use iroh::EndpointAddr;
use storage_service::Residency;

use crate::{LocalSetupJoin, RouterState};

const SETUP_TOKEN_HEADER: &str = "x-setup-token";

fn setup_services(
    state: &RouterState,
) -> Result<&super::super::state::LocalSetupServices, ErrorResponse> {
    state
        .local_setup
        .as_ref()
        .ok_or_else(|| ErrorResponse::new(ErrorCode::ServerError))
}

fn authorize<'a>(
    state: &'a RouterState,
    headers: &HeaderMap,
) -> Result<&'a super::super::state::LocalSetupServices, ErrorResponse> {
    let setup = setup_services(state)?;
    let token = headers
        .get(SETUP_TOKEN_HEADER)
        .and_then(|token| token.to_str().ok());
    if setup.setup.authorize(token) {
        Ok(setup)
    } else {
        Err(ErrorResponse::new(ErrorCode::NotAuthorized))
    }
}

fn require_installation(stage: SetupStage) -> Result<(), ErrorResponse> {
    if stage == SetupStage::Installation {
        Ok(())
    } else {
        Err(ErrorResponse::new(ErrorCode::InvalidRequest))
    }
}

fn endpoint_addr(value: &str) -> Result<String, ErrorResponse> {
    let endpoint_addr: EndpointAddr =
        serde_json::from_str(value).map_err(|_| ErrorResponse::new(ErrorCode::InvalidRequest))?;
    if endpoint_addr.is_empty() {
        return Err(ErrorResponse::new(ErrorCode::InvalidRequest));
    }
    serde_json::to_string(&endpoint_addr).map_err(|_| ErrorResponse::new(ErrorCode::InvalidRequest))
}

#[utoipa::path(
    get,
    path = "/setup/status",
    responses((status = 200, description = "Local setup status", body = SetupStatus))
)]
pub(crate) async fn setup_status(
    State(state): State<RouterState>,
) -> Result<Json<SetupStatus>, ErrorResponse> {
    Ok(Json(SetupStatus {
        stage: setup_services(&state)?.setup.stage(),
    }))
}

#[utoipa::path(
    post,
    path = "/setup/new",
    params(("x-setup-token" = String, Header, description = "Setup token")),
    request_body = SetupNewRequest,
    responses((status = 200, description = "Baseline created", body = SetupStatus))
)]
pub(crate) async fn setup_new(
    State(state): State<RouterState>,
    headers: HeaderMap,
    Json(request): Json<SetupNewRequest>,
) -> Result<Json<SetupStatus>, ErrorResponse> {
    let setup = authorize(&state, &headers)?;
    require_installation(setup.setup.stage())?;

    let device = (
        state.device_identity.endpoint_id().to_string(),
        state
            .device_identity
            .endpoint_address()
            .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?,
    );
    let input = BootstrapInput {
        device_name: request.device_name,
        admin_username: request.admin_username,
        admin_password: request.admin_password,
    };
    state
        .setup_new_executor
        .executor()
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?
        .bootstrap(input, device)
        .await
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?;
    setup
        .setup
        .advance(SetupStage::Device)
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?;

    Ok(Json(SetupStatus {
        stage: SetupStage::Device,
    }))
}

#[utoipa::path(
    post,
    path = "/setup/join",
    params(("x-setup-token" = String, Header, description = "Setup token")),
    request_body = SetupJoinRequest,
    responses((status = 200, description = "Join request persisted; global sync is pending", body = SetupJoinStatus))
)]
pub(crate) async fn setup_join(
    State(state): State<RouterState>,
    headers: HeaderMap,
    Json(request): Json<SetupJoinRequest>,
) -> Result<Json<SetupJoinStatus>, ErrorResponse> {
    let setup = authorize(&state, &headers)?;
    require_installation(setup.setup.stage())?;
    let endpoint_addr = endpoint_addr(&request.endpoint_addr)?;
    let joining_public_key = state.device_identity.endpoint_id().to_string();
    let nonce = setup
        .setup
        .join()
        .filter(|join| {
            join.device_name == request.device_name
                && join.endpoint_addr == endpoint_addr
                && join.joining_public_key == joining_public_key
                && !join.nonce.is_empty()
        })
        .map_or_else(generate_random_string::<32>, |join| join.nonce);
    let join = LocalSetupJoin {
        device_name: request.device_name,
        endpoint_addr,
        joining_public_key,
        nonce,
    };
    setup
        .setup
        .save_join(join.clone())
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?;
    let executor = state
        .setup_join_executor
        .executor()
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?;
    executor
        .join(Arc::clone(&setup.setup), join)
        .await
        .map_err(|_| ErrorResponse::new(ErrorCode::InvalidRequest))?;

    Ok(Json(SetupJoinStatus {
        state: SetupJoinState::Complete,
    }))
}

#[utoipa::path(
    get,
    path = "/setup/device/residency",
    params(("x-setup-token" = String, Header, description = "Setup token")),
    responses((status = 200, description = "Device default residency", body = SetupDeviceStatus))
)]
pub(crate) async fn device_residency(
    State(state): State<RouterState>,
    headers: HeaderMap,
) -> Result<Json<SetupDeviceStatus>, ErrorResponse> {
    let setup = authorize(&state, &headers)?;
    if setup.setup.stage() != SetupStage::Device {
        return Err(ErrorResponse::new(ErrorCode::InvalidRequest));
    }
    let residency = match setup
        .residency_policy
        .lock()
        .expect("residency policy lock poisoned")
        .device_default()
    {
        Residency::Full => SetupResidency::Full,
        Residency::Passthrough => SetupResidency::Passthrough,
    };
    Ok(Json(SetupDeviceStatus { residency }))
}

#[utoipa::path(
    put,
    path = "/setup/device/residency",
    params(("x-setup-token" = String, Header, description = "Setup token")),
    request_body = SetupDeviceRequest,
    responses((status = 200, description = "Device default residency", body = SetupDeviceStatus))
)]
pub(crate) async fn set_device_residency(
    State(state): State<RouterState>,
    headers: HeaderMap,
    Json(request): Json<SetupDeviceRequest>,
) -> Result<Json<SetupDeviceStatus>, ErrorResponse> {
    let setup = authorize(&state, &headers)?;
    if setup.setup.stage() != SetupStage::Device {
        return Err(ErrorResponse::new(ErrorCode::InvalidRequest));
    }
    let residency = match request.residency {
        SetupResidency::Full => Residency::Full,
        SetupResidency::Passthrough => Residency::Passthrough,
    };
    let mut policy = setup
        .residency_policy
        .lock()
        .expect("residency policy lock poisoned");
    policy.set_device_default(residency);
    policy
        .save(&setup.residency_root)
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?;
    Ok(Json(SetupDeviceStatus {
        residency: request.residency,
    }))
}

#[utoipa::path(
    post,
    path = "/setup/device",
    params(("x-setup-token" = String, Header, description = "Setup token")),
    responses((status = 200, description = "Setup complete", body = SetupStatus))
)]
pub(crate) async fn complete_device_setup(
    State(state): State<RouterState>,
    headers: HeaderMap,
) -> Result<Json<SetupStatus>, ErrorResponse> {
    let setup = authorize(&state, &headers)?;
    if setup.setup.stage() != SetupStage::Device {
        return Err(ErrorResponse::new(ErrorCode::InvalidRequest));
    }
    setup
        .setup
        .advance(SetupStage::Ready)
        .map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?;

    Ok(Json(SetupStatus {
        stage: SetupStage::Ready,
    }))
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use iroh::{EndpointAddr, SecretKey};

    use super::{endpoint_addr, require_installation};
    use idp_model::contract::{ErrorCode, SetupStage};

    #[test]
    fn accepts_serialized_reachable_endpoint_address() {
        let address = EndpointAddr::new(SecretKey::generate().public()).with_ip_addr(
            "127.0.0.1:11204"
                .parse::<SocketAddr>()
                .expect("valid socket address"),
        );
        let serialized = serde_json::to_string(&address).expect("serializes endpoint address");

        assert_eq!(
            endpoint_addr(&serialized).expect("accepts endpoint address"),
            serialized
        );
    }

    #[test]
    fn rejects_unusable_endpoint_address_and_non_installation_stages() {
        assert_eq!(
            endpoint_addr("{}")
                .expect_err("rejects invalid endpoint")
                .error,
            ErrorCode::InvalidRequest
        );
        assert_eq!(
            endpoint_addr(
                &serde_json::to_string(&EndpointAddr::new(SecretKey::generate().public()))
                    .expect("serializes endpoint address")
            )
            .expect_err("rejects empty endpoint")
            .error,
            ErrorCode::InvalidRequest
        );
        assert!(require_installation(SetupStage::Installation).is_ok());
        assert_eq!(
            require_installation(SetupStage::Device)
                .expect_err("rejects device stage")
                .error,
            ErrorCode::InvalidRequest
        );
    }
}
