use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use bootstrap_service::bootstrap::{BootstrapConfig, BootstrapInput, BootstrapService};
use db::{
    EnvelopeOutcome, NativeEngine, frontier_bytes, import_checkpoint_bytes, import_envelope_bytes,
};
use idp_model::contract::{
    DeviceState, SetupJoinRequest, SetupNewRequest, SetupStage, SetupStatus,
};
use idp_server::{AppConfig, DeviceIdentity};
use idp_service::{
    replica::{DbApplicationRepo, DbClientRepo, DbKeyRepo, DbUserRepo},
    repo::{KeyService, PrivateKeyKeyringRepo},
};
use management_service::{
    DeviceRepo,
    replica::{DbDeviceRepo, DbPermissionRepo, DbRoleRepo},
};

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
    let database = state.database.clone();
    let devices = DbDeviceRepo::new(database.clone());
    if devices
        .has_any()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        return Err(StatusCode::CONFLICT);
    }

    if request.device_name.trim().is_empty()
        || request.admin_username.trim().is_empty()
        || request.admin_password.is_empty()
    {
        return Err(StatusCode::BAD_REQUEST);
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

    let address = state
        .device_identity
        .endpoint_address()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let client = reqwest::Client::new();
    let base_url = request.idp_url.trim_end_matches('/');
    let device_response = client
        .post(format!("{base_url}/setup/devices"))
        .header("authorization", authorization)
        .json(&serde_json::json!({
            "deviceName": request.device_name,
            "publicKey": state.device_identity.endpoint_id().to_string(),
            "address": address,
        }))
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !device_response.status().is_success() {
        return Err(match device_response.status().as_u16() {
            401 | 403 => StatusCode::UNAUTHORIZED,
            409 => StatusCode::CONFLICT,
            _ => StatusCode::BAD_GATEWAY,
        });
    }
    let device: idp_model::contract::SetupDeviceAuthorizationResponse = device_response
        .json()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let sync_response = client
        .post(format!("{base_url}/setup/sync"))
        .header("authorization", authorization)
        .json(&idp_model::contract::SetupSyncRequest {
            device_id: device.device_id,
            frontier: None,
        })
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !sync_response.status().is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }
    let sync: idp_model::contract::SetupSyncResponse = sync_response
        .json()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let checkpoint = sync.checkpoint.ok_or(StatusCode::BAD_GATEWAY)?;
    import_checkpoint_bytes(&state.database, &checkpoint)
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let local_public_key = state.device_identity.endpoint_id().to_string();
    let devices = DbDeviceRepo::new(state.database.clone());
    let approved = devices
        .list()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .into_iter()
        .any(|device| {
            device.public_key == local_public_key && device.state == DeviceState::Approved
        });
    if !approved {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let incremental_response = client
        .post(format!("{base_url}/setup/sync"))
        .header("authorization", authorization)
        .json(&idp_model::contract::SetupSyncRequest {
            device_id: device.device_id,
            frontier: Some(sync.frontier),
        })
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !incremental_response.status().is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }
    let incremental: idp_model::contract::SetupSyncResponse = incremental_response
        .json()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    for envelope in incremental.envelopes {
        if matches!(
            import_envelope_bytes(&state.database, envelope)
                .await
                .map_err(|_| StatusCode::BAD_GATEWAY)?,
            EnvelopeOutcome::Quarantined { .. }
        ) {
            return Err(StatusCode::BAD_GATEWAY);
        }
    }

    let local_frontier = frontier_bytes(&state.database)
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !frontier_is_complete(&local_frontier, &incremental.frontier) {
        return Err(StatusCode::BAD_GATEWAY);
    }

    Ok(StatusCode::NO_CONTENT)
}

fn frontier_is_complete(local: &[u8], remote: &[u8]) -> bool {
    local == remote
}

#[cfg(test)]
mod tests {
    use super::frontier_is_complete;

    #[test]
    fn join_requires_the_local_frontier_to_match_the_server() {
        assert!(frontier_is_complete(b"frontier", b"frontier"));
        assert!(!frontier_is_complete(b"local", b"remote"));
    }
}
