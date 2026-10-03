use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use bootstrap_service::bootstrap::{BootstrapConfig, BootstrapInput, BootstrapService};
use db::NativeEngine;
use idp_model::contract::{
    DeviceState, SetupBootstrapRegistration, SetupBootstrapRequest, SetupJoinRequest,
    SetupNewRequest, SetupStage, SetupStatus,
};
use idp_server::{AppConfig, DeviceIdentity};
use idp_service::{
    replica::{DbApplicationRepo, DbClientRepo, DbKeyRepo, DbUserRepo},
    repo::{KeyService, PrivateKeyKeyringRepo},
};
use iroh::EndpointAddr;
use management_service::{
    DeviceRepo,
    replica::{DbDeviceRepo, DbPermissionRepo, DbRoleRepo},
};
use ofdb_sql::{IrohTransport, SessionConfig, SyncRole};

#[derive(Clone)]
pub struct SetupState {
    pub database: Arc<NativeEngine>,
    pub app_config: Arc<AppConfig>,
    pub device_identity: Arc<DeviceIdentity>,
}

pub fn router(state: SetupState) -> Router {
    Router::new()
        .route("/setup/status", get(status))
        .route("/setup/new", post(create_system))
        .route("/setup/join", post(join_system))
        .with_state(state)
}

async fn status(State(state): State<SetupState>) -> Result<Json<SetupStatus>, StatusCode> {
    if state
        .database
        .table_names()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .is_empty()
    {
        return Ok(Json(SetupStatus {
            stage: SetupStage::Installation,
        }));
    }
    let devices = DbDeviceRepo::new(state.database);
    let devices = devices
        .list()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let stage = if devices
        .iter()
        .any(|device| device.state == DeviceState::Approved)
    {
        SetupStage::Ready
    } else if devices.is_empty() {
        SetupStage::Installation
    } else {
        SetupStage::Device
    };
    Ok(Json(SetupStatus { stage }))
}

async fn create_system(
    State(state): State<SetupState>,
    Json(request): Json<SetupNewRequest>,
) -> Result<StatusCode, StatusCode> {
    if request.device_name.trim().is_empty()
        || request.admin_username.trim().is_empty()
        || request.admin_password.is_empty()
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    idp_model::replica::up(&state.database)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let database = state.database.clone();
    let devices = DbDeviceRepo::new(database.clone());
    if devices
        .has_any()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        return Err(StatusCode::CONFLICT);
    }

    let key_service = Arc::new(KeyService::new(
        DbKeyRepo::new(database.clone()),
        PrivateKeyKeyringRepo::new(&state.app_config.oauth2.issuer),
        state.app_config.key_namespace.clone(),
    ));
    let bootstrap = BootstrapService::new(
        DbApplicationRepo::new(database.clone()),
        DbClientRepo::new(database.clone(), key_service.clone()),
        DbUserRepo::new(database.clone(), state.app_config.password.clone()),
        DbRoleRepo::new(database.clone()),
        DbPermissionRepo::new(database.clone()),
        DbDeviceRepo::new(database),
        key_service,
        BootstrapConfig {
            web: false,
            desktop: true,
            idp_url: state.app_config.ui_public_uri.clone(),
            management_url: state.app_config.api_public_uri.clone(),
        },
    );

    let address = state
        .device_identity
        .endpoint_address()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    bootstrap
        .ensure_system_baseline(
            &BootstrapInput {
                device_name: request.device_name,
                admin_username: request.admin_username,
                admin_password: request.admin_password,
            },
            Some((state.device_identity.endpoint_id().to_string(), address)),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::NO_CONTENT)
}

const BOOTSTRAP_ALPN: &[u8] = b"idp-bootstrap/1";

async fn join_system(
    State(state): State<SetupState>,
    headers: HeaderMap,
    Json(request): Json<SetupJoinRequest>,
) -> Result<StatusCode, StatusCode> {
    if request.device_name.trim().is_empty() || request.idp_url.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let authorization = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.starts_with("Bearer "))
        .ok_or(StatusCode::UNAUTHORIZED)?;
    let endpoint_id = state.device_identity.endpoint_id();
    let endpoint_addr = state
        .device_identity
        .endpoint_address()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let client = reqwest::Client::new();
    let base_url = request.idp_url.trim_end_matches('/');
    let registration = client
        .post(format!("{base_url}/setup/bootstrap"))
        .header("authorization", authorization)
        .json(&SetupBootstrapRequest {
            device_name: request.device_name,
            endpoint_id: endpoint_id.to_string(),
            endpoint_addr,
        })
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !registration.status().is_success() {
        return Err(match registration.status().as_u16() {
            401 | 403 => StatusCode::UNAUTHORIZED,
            409 => StatusCode::CONFLICT,
            _ => StatusCode::BAD_GATEWAY,
        });
    }
    let registration: SetupBootstrapRegistration = registration
        .json()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let remote_id = registration
        .endpoint_id
        .parse::<iroh::EndpointId>()
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let remote_addr = serde_json::from_str::<EndpointAddr>(&registration.endpoint_addr)
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if remote_addr.id != remote_id {
        return Err(StatusCode::BAD_GATEWAY);
    }

    let endpoint = state.device_identity.endpoint();
    endpoint.set_alpns(vec![BOOTSTRAP_ALPN.to_vec()]);
    let connection = endpoint
        .connect(remote_addr, BOOTSTRAP_ALPN)
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let (mut send, mut recv) = connection
        .open_bi()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let grant = registration.grant.as_bytes();
    let length = u16::try_from(grant.len()).map_err(|_| StatusCode::BAD_GATEWAY)?;
    send.write_all(&length.to_be_bytes())
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    send.write_all(grant)
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let mut acknowledgement = [0; 2];
    recv.read_exact(&mut acknowledgement)
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if &acknowledgement != b"OK" {
        return Err(StatusCode::BAD_GATEWAY);
    }
    let mut transport = IrohTransport::new(send, recv);
    ofdb_sql::synchronize(
        &state.database,
        &mut transport,
        &SessionConfig::default(),
        SyncRole::Initiator,
    )
    .await
    .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let devices = DbDeviceRepo::new(state.database.clone());
    let approved = devices
        .list()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .any(|device| {
            device.public_key == endpoint_id.to_string() && device.state == DeviceState::Approved
        });
    if !approved {
        return Err(StatusCode::BAD_GATEWAY);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
#[path = "setup_tests.rs"]
mod setup_tests;
