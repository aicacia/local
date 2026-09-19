use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use axum::Router;

use db::{close_database, open_database};
use idp_model::contract::{DeviceSelfRevocationRequest, device_self_revocation_payload};
use idp_server::{
    ActiveGlobalIdentityReadGate, AppConfig, DeviceIdentity, GlobalIdentityCache,
    GlobalIdentityJoinApprover, LocalSetupState, RouterState, TimedPairingAcceptanceController,
    delete_device_identity, open_device_identity, storage_router,
};
use idp_service::libsql::{
    LibSqlApplicationRepo, LibSqlClientRepo, LibSqlKeyRepo, LibSqlOAuth2AuthorizationCodeRepo,
    LibSqlOAuth2UserConsentRepo, LibSqlUserRepo,
};
use idp_service::{
    generate_random_string,
    oauth2::OAuth2Service,
    repo::{KeyService, PrivateKeyKeyringRepo},
};
use libsql::Database;
use management_service::libsql::{LibSqlDeviceRepo, LibSqlPermissionRepo, LibSqlRoleRepo};
use management_service::{DeviceRepo, ManagementService};
use tauri::{AppHandle, Manager, Wry, async_runtime::Mutex};
use tokio::{net::TcpListener, time::timeout};
use tower_http::cors::CorsLayer;

use crate::localhost_server::{
    LocalhostServer, localhost_server_base_url, reserve_localhost_listener,
    start_unified_localhost_server,
};
use crate::{
    global_identity::{DesktopGlobalRuntime, DesktopSetupJoinExecutor, DesktopSetupNewExecutor},
    hosted_control_plane::HostedControlPlane,
    local_api,
    scoped_transport::AppFileSystemRuntime,
    tunnel_authorizer::{DeviceTunnelManager, LidpTunnelAuthorizer},
};

#[derive(Clone, Default)]
pub struct LocalhostServerState {
    pub base_url: String,
    pub ready: bool,
}

pub fn init_router(
    app_config: Arc<AppConfig>,
    database: Arc<Database>,
    file_systems: Arc<AppFileSystemRuntime>,
    device_identity: Arc<DeviceIdentity>,
    setup_state: LocalSetupState,
    setup_data_dir: impl AsRef<Path>,
    control_plane: Option<Arc<HostedControlPlane>>,
) -> io::Result<(Router, Arc<RouterState>)> {
    let key_service = Arc::new(KeyService::new(
        LibSqlKeyRepo::new(database.clone()),
        PrivateKeyKeyringRepo::new(&app_config.oauth2.issuer),
        app_config.key_namespace.clone(),
    ));

    let oauth2_service = Arc::new(OAuth2Service::new(
        LibSqlApplicationRepo::new(database.clone()),
        LibSqlClientRepo::new(database.clone(), key_service.clone()),
        LibSqlOAuth2AuthorizationCodeRepo::new(database.clone()),
        LibSqlUserRepo::new(
            database.clone(),
            key_service.clone(),
            app_config.password.clone(),
        ),
        LibSqlOAuth2UserConsentRepo::new(database.clone()),
        key_service.clone(),
        app_config.oauth2.clone(),
    ));

    let router_state = RouterState::new(
        &app_config.ui_public_uri,
        &app_config.api_public_uri,
        database.clone(),
        oauth2_service.clone(),
        Arc::new(LibSqlDeviceRepo::new(database.clone())),
        device_identity,
    )
    .with_local_setup(setup_data_dir.as_ref().to_path_buf(), setup_state)
    .with_storage_file_systems(Arc::clone(&file_systems));
    let router_state = Arc::new(match control_plane {
        Some(control_plane) => router_state.with_hosted_control_plane(control_plane),
        None => router_state,
    });
    let idp_router = idp_server::openapi_router(router_state.as_ref().clone(), "/lidp");
    let management_service = Arc::new(ManagementService::new(
        LibSqlApplicationRepo::new(database.clone()),
        LibSqlPermissionRepo::new(database.clone()),
        LibSqlRoleRepo::new(database.clone()),
    ));
    let management_router = management_server::openapi_router(
        management_server::RouterState::new(
            &app_config.api_public_uri,
            database,
            Arc::clone(&router_state.global_identity_read_gate),
            management_service,
            oauth2_service,
        ),
        "/idp-management",
    );

    let storage_router = storage_router(router_state.as_ref().clone(), file_systems);
    Ok((
        idp_router
            .split_for_parts()
            .0
            .merge(management_router.split_for_parts().0)
            .merge(storage_router)
            .merge(local_api::router())
            .layer(CorsLayer::very_permissive().allow_private_network(true)),
        router_state,
    ))
}

pub async fn init_database(
    app_handle: AppHandle<Wry>,
    app_config: Arc<AppConfig>,
) -> io::Result<Arc<Database>> {
    let database = Arc::new(open_database(&app_config.database).await.map_err(|e| {
        log::error!("failed to create database pool: {e}");
        io::Error::other(e)
    })?);

    idp_model::migrate::up(&database).await.map_err(|e| {
        log::error!("failed to run database migrations: {e}");
        io::Error::other(e)
    })?;

    app_handle.manage(database.clone());

    Ok(database)
}

pub fn init_app_config(
    app_handle: &AppHandle<Wry>,
    data_dir: impl AsRef<Path>,
) -> tauri::Result<Arc<AppConfig>> {
    if !data_dir.as_ref().exists() {
        fs::create_dir_all(&data_dir)?;
    }

    let config_path = data_dir.as_ref().join("config.yaml");
    let mut app_config = if config_path.exists() {
        AppConfig::try_from(config_path.as_path())
            .map_err(|e| tauri::Error::Io(io::Error::other(e)))?
    } else {
        let mut default_config = AppConfig::default();
        default_config.bootstrap.web = false;
        default_config.bootstrap.desktop = true;
        default_config.data_dir = data_dir.as_ref().to_string_lossy().into_owned();
        default_config.database.url = format!(
            "file://{}",
            data_dir.as_ref().join("lidp.db").to_string_lossy()
        );
        default_config.oauth2.issuer = "https://localhost".to_owned();
        default_config.ui_public_uri = "https://localhost".to_owned();
        default_config.api_public_uri = "https://localhost".to_owned();
        fs::write(
            &config_path,
            yaml_serde::to_string(&default_config)
                .map_err(|e| tauri::Error::Io(io::Error::other(e)))?,
        )?;
        default_config
    };
    app_config.data_dir = data_dir.as_ref().to_string_lossy().into_owned();

    let app_config = Arc::new(app_config);
    app_handle.manage(app_config.clone());
    Ok(app_config)
}

#[tauri::command]
pub async fn get_localhost_server_base_url(app_handle: AppHandle<Wry>) -> String {
    localhost_server_base_url_for(&app_handle).await
}

#[tauri::command]
pub fn get_setup_token(app_handle: AppHandle<Wry>) -> Option<String> {
    app_handle
        .try_state::<Arc<RouterState>>()
        .and_then(|state| state.setup_token())
}

#[tauri::command]
pub async fn reset_device(app_handle: AppHandle<Wry>) -> Result<(), String> {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let config_dir = app_handle
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?;
    let setup_state = app_handle
        .try_state::<LocalSetupState>()
        .ok_or_else(|| "setup state is missing".to_owned())?
        .inner()
        .clone();
    let app_config = app_handle
        .try_state::<Arc<AppConfig>>()
        .ok_or_else(|| "app configuration is missing".to_owned())?
        .inner()
        .clone();

    if let Some(router_state) = app_handle.try_state::<Arc<RouterState>>() {
        let public_key = router_state.device_identity.endpoint_id().to_string();
        let request = DeviceSelfRevocationRequest {
            signature: router_state
                .device_identity
                .sign(device_self_revocation_payload(&public_key).as_bytes()),
            public_key: public_key.clone(),
        };
        if let Some(control_plane) = &router_state.hosted_control_plane {
            let _ = timeout(Duration::from_secs(2), control_plane.revoke_self(request)).await;
        } else {
            let _ = timeout(
                Duration::from_secs(2),
                router_state.devices.revoke_self(&public_key),
            )
            .await;
        }
    }

    close(&app_handle)
        .await
        .map_err(|error| error.to_string())?;
    delete_device_identity(&setup_state).map_err(|error| error.to_string())?;
    remove_local_reset_data(&data_dir, &config_dir, &app_config.database.url)
        .map_err(|error| error.to_string())?;
    app_handle.exit(0);
    Ok(())
}

fn remove_local_reset_data(
    data_dir: &Path,
    config_dir: &Path,
    database_url: &str,
) -> io::Result<()> {
    remove_path(&LocalSetupState::path(data_dir))?;
    remove_path(&data_dir.join("vaults"))?;
    remove_path(&data_dir.join("global-identity"))?;
    remove_path(&data_dir.join("storage-residency.json"))?;

    let default_database = config_dir.join("lidp.db");
    if database_url == format!("file://{}", default_database.to_string_lossy()) {
        remove_path(&default_database)?;
        remove_path(&PathBuf::from(format!(
            "{}-shm",
            default_database.to_string_lossy()
        )))?;
        remove_path(&PathBuf::from(format!(
            "{}-wal",
            default_database.to_string_lossy()
        )))?;
    }
    Ok(())
}

fn remove_path(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub async fn localhost_server_base_url_for(app_handle: &AppHandle<Wry>) -> String {
    if let Some(state) = app_handle.try_state::<Mutex<LocalhostServerState>>() {
        let state = state.lock().await;
        if state.ready {
            state.base_url.clone()
        } else {
            String::new()
        }
    } else {
        String::new()
    }
}

pub async fn set_localhost_server_state(
    app_handle: &AppHandle<Wry>,
    base_url: String,
    ready: bool,
) {
    if let Some(state) = app_handle.try_state::<Mutex<LocalhostServerState>>() {
        *state.lock().await = LocalhostServerState { base_url, ready };
    } else {
        app_handle.manage(Mutex::new(LocalhostServerState { base_url, ready }));
    }
}

pub async fn init_scoped_file_system_runtime(
    app_handle: &AppHandle<Wry>,
    _: &AppConfig,
) -> tauri::Result<()> {
    let data_dir = app_handle.path().app_data_dir()?;
    let local_peer = app_handle
        .try_state::<Arc<DeviceIdentity>>()
        .ok_or_else(|| tauri::Error::Io(io::Error::other("device identity is missing")))?
        .endpoint_id();
    let runtime = AppFileSystemRuntime::new(data_dir, local_peer).map_err(tauri::Error::Io)?;
    app_handle.manage(Arc::new(runtime));
    Ok(())
}

pub async fn init_device_identity(app_handle: &AppHandle<Wry>) -> tauri::Result<()> {
    let setup_state = LocalSetupState::load_or_create(app_handle.path().app_data_dir()?)?;
    let identity = open_device_identity(&setup_state).await?;
    app_handle.manage(setup_state);
    app_handle.manage(Arc::new(identity));
    Ok(())
}

pub async fn init_tunnel_manager(
    app_handle: &AppHandle<Wry>,
    state: Arc<RouterState>,
    control_plane: Option<Arc<HostedControlPlane>>,
) -> tauri::Result<()> {
    let identity = app_handle
        .try_state::<Arc<DeviceIdentity>>()
        .ok_or_else(|| tauri::Error::Io(io::Error::other("device identity is missing")))?;
    let app_config = app_handle
        .try_state::<Arc<AppConfig>>()
        .ok_or_else(|| tauri::Error::Io(io::Error::other("app config is missing")))?;
    let allowlist = iroh_chain::DynamicEndpointIdStore::default();
    let global_grants = Arc::new(idp_server::GlobalBootstrapGrants::new());
    let authorizer = LidpTunnelAuthorizer::new(
        Arc::clone(&state),
        control_plane.clone(),
        Arc::clone(&global_grants),
    );
    let manager = Arc::new(DeviceTunnelManager::new(
        identity.endpoint(),
        allowlist.clone(),
        authorizer,
    ));
    state
        .pairing_acceptance
        .bind(Arc::new(TimedPairingAcceptanceController::new(
            (*manager).clone(),
            Duration::from_secs(app_config.pairing.accepting_timeout_seconds),
        )))
        .map_err(|error| tauri::Error::Io(io::Error::other(error)))?;
    let global_transport = iroh_chain_file_system::ScopedIrohTransport::new(
        (*manager).clone(),
        iroh_chain::VaultId::global_identity(),
        iroh_chain_file_system::StaticTunnelAuthorization::new(Vec::new()),
    );
    let global_runtime = Arc::new(
        DesktopGlobalRuntime::new(
            PathBuf::from(&app_config.data_dir),
            identity.endpoint_id(),
            global_transport,
        )
        .await
        .map_err(|error| tauri::Error::Io(io::Error::other(error)))?,
    );
    let global_cache = GlobalIdentityCache::new(Arc::clone(&state.database));
    state
        .global_identity_read_gate
        .bind(Arc::new(ActiveGlobalIdentityReadGate::new(
            Arc::clone(&global_runtime),
            global_cache.clone(),
        )))
        .map_err(|error| tauri::Error::Io(io::Error::other(error)))?;
    let join_approver = Arc::new(GlobalIdentityJoinApprover::new(
        Arc::clone(&global_runtime),
        global_cache.clone(),
        Arc::clone(&global_grants),
        identity.endpoint_id().to_string(),
        identity
            .endpoint_address()
            .map_err(|error| tauri::Error::Io(io::Error::other(error)))?,
    ));
    let mut pairing_offers = manager.subscribe_pairing_offers();
    let pairing_allowlist = allowlist.clone();
    let pairing_runtime = Arc::clone(&global_runtime);
    let pairing_cache = global_cache.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let Ok(pairing_offer) = pairing_offers.recv().await else {
                return;
            };
            let Ok(offer) = serde_json::from_slice::<idp_model::contract::GlobalIdentityJoinOffer>(
                &pairing_offer.payload,
            ) else {
                continue;
            };
            if offer.joining_public_key != pairing_offer.remote_id.to_string() {
                continue;
            }
            let Ok(Some((manifest, _))) = pairing_runtime.active_rows().await else {
                continue;
            };
            let Ok(Some(cache_revision)) = pairing_cache.revision().await else {
                continue;
            };
            if cache_revision != manifest.revision {
                continue;
            }
            pairing_allowlist
                .insert_scope("global-identity".to_owned(), pairing_offer.remote_id)
                .await;
            let expires_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |duration| {
                    i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
                })
                .saturating_add(300);
            let Ok(reply) = join_approver
                .approve(
                    &pairing_offer.remote_id.to_string(),
                    offer,
                    generate_random_string::<32>(),
                    expires_at,
                )
                .await
            else {
                continue;
            };
            let Ok(reply) = serde_json::to_vec(&reply) else {
                continue;
            };
            let _ = pairing_offer.reply(&reply).await;
        }
    });
    state
        .setup_new_executor
        .bind(Arc::new(DesktopSetupNewExecutor::new(
            Arc::clone(&global_runtime),
            global_cache.clone(),
            PathBuf::from(&app_config.data_dir),
            app_config.bootstrap.clone(),
            app_config.password.clone(),
            app_config.oauth2.issuer.clone(),
            app_config.key_namespace.clone(),
        )))
        .map_err(|error| tauri::Error::Io(io::Error::other(error)))?;
    state
        .setup_join_executor
        .bind(Arc::new(DesktopSetupJoinExecutor::new(
            (*manager).clone(),
            allowlist.clone(),
            global_cache,
            PathBuf::from(&app_config.data_dir),
        )))
        .map_err(|error| tauri::Error::Io(io::Error::other(error)))?;
    let listener = Arc::clone(&manager);
    tauri::async_runtime::spawn(async move {
        listener.listen().await;
    });
    let allowlist = Arc::new(allowlist);

    app_handle.manage(allowlist);
    app_handle.manage(global_grants);
    app_handle.manage(manager);
    Ok(())
}

pub async fn init_unified_localhost_server(
    app_handle: &AppHandle<Wry>,
    router: Router,
    listener: TcpListener,
    base_url: String,
) -> tauri::Result<String> {
    let data_dir = app_handle.path().app_data_dir()?;
    let server = start_unified_localhost_server(router, listener, &data_dir);
    app_handle.manage(Mutex::new(Some(server)));

    set_localhost_server_state(app_handle, base_url.clone(), true).await;
    Ok(base_url)
}

pub async fn reserve_unified_localhost_server(
    app_handle: &AppHandle<Wry>,
) -> tauri::Result<(TcpListener, String)> {
    let app_data_dir = app_handle.path().app_data_dir()?;
    let (listener, port) = reserve_localhost_listener(&app_data_dir)
        .await
        .map_err(|err| tauri::Error::Io(io::Error::other(err)))?;
    Ok((listener, localhost_server_base_url(port)))
}

pub fn app_config_for_localhost_base_url(
    app_config: Arc<AppConfig>,
    base_url: &str,
) -> Arc<AppConfig> {
    let mut config = app_config.as_ref().clone();
    config.oauth2.issuer = format!("{base_url}/lidp");
    config.ui_public_uri = base_url.to_owned();
    config.api_public_uri = base_url.to_owned();
    config.bootstrap.idp_url = format!("{base_url}/lidp");
    config.bootstrap.management_url = format!("{base_url}/idp-management");
    Arc::new(config)
}

pub async fn close(app_handle: &AppHandle<Wry>) -> io::Result<()> {
    set_localhost_server_state(app_handle, String::new(), false).await;
    if let Some(server) = app_handle.try_state::<Mutex<Option<LocalhostServer>>>() {
        if let Some(server) = server.lock().await.take() {
            server.close().await?;
        }
    }
    if let Some(manager) = app_handle.try_state::<Arc<DeviceTunnelManager>>() {
        manager.close().await;
    }
    if let Some(database) = app_handle.try_state::<Arc<Database>>() {
        close_database(database.inner())
            .await
            .map_err(io::Error::other)?;
    }
    Ok(())
}
