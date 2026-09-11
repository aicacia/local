use std::{fs, io, path::Path, sync::Arc, time::Duration};

use axum::Router;
use db::{close_database, open_database};
use libsql::Database;
use lidp_server::{AppConfig, RouterState, TimedPairingAcceptanceController, storage_router};
use lidp_service::{
    bootstrap::BootstrapService,
    management::ManagementService,
    oauth2::OAuth2Service,
    repo::{
        KeyService, LibSqlApplicationRepo, LibSqlClientRepo, LibSqlDeviceRepo, LibSqlKeyRepo,
        LibSqlOAuth2AuthorizationCodeRepo, LibSqlOAuth2UserConsentRepo, LibSqlPermissionRepo,
        LibSqlRoleRepo, LibSqlUserRepo, PrivateKeyKeyringRepo,
    },
    storage_session::StorageSessionService,
};
use tauri::{AppHandle, Manager, Wry, async_runtime::Mutex};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;

use crate::localhost_server::{
    localhost_server_base_url, reserve_localhost_listener, start_unified_localhost_server,
};
use crate::{
    device_identity::{DeviceIdentity, open as open_device_identity},
    hosted_control_plane::HostedControlPlane,
    local_api,
    scoped_transport::{AppFileSystemRuntime, TunnelContext},
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
        key_service,
        app_config.oauth2.clone(),
    ));

    let storage_sessions = Arc::new(StorageSessionService::new());
    let router_state = RouterState::new(
        &app_config.ui_public_uri,
        &app_config.api_public_uri,
        database.clone(),
        oauth2_service.clone(),
        storage_sessions.clone(),
        Arc::new(LibSqlDeviceRepo::new(database.clone())),
        device_identity,
    );
    let router_state = Arc::new(match control_plane {
        Some(control_plane) => router_state.with_storage_scope_resolver(Arc::new(
            lidp_server::HostedStorageScopeResolver::new(control_plane),
        )),
        None => router_state,
    });
    let lidp_router = lidp_server::openapi_router(router_state.as_ref().clone(), "/lidp");
    let management_service = Arc::new(ManagementService::new(
        LibSqlApplicationRepo::new(database.clone()),
        LibSqlPermissionRepo::new(database.clone()),
        LibSqlRoleRepo::new(database.clone()),
    ));
    let management_router = lidp_management_server::openapi_router(
        lidp_management_server::RouterState::new(
            &app_config.api_public_uri,
            database,
            management_service,
            oauth2_service,
        ),
        "/lidp-management",
    );

    let storage_router = storage_router(storage_sessions, file_systems);
    Ok((
        lidp_router
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

    lidp_model::migrate::up(&database).await.map_err(|e| {
        log::error!("failed to run database migrations: {e}");
        io::Error::other(e)
    })?;

    let key_service = Arc::new(KeyService::new(
        LibSqlKeyRepo::new(database.clone()),
        PrivateKeyKeyringRepo::new(&app_config.oauth2.issuer),
        app_config.key_namespace.clone(),
    ));
    let bootstrap_service = BootstrapService::new(
        LibSqlApplicationRepo::new(database.clone()),
        LibSqlClientRepo::new(database.clone(), key_service.clone()),
        LibSqlUserRepo::new(
            database.clone(),
            key_service.clone(),
            app_config.password.clone(),
        ),
        LibSqlRoleRepo::new(database.clone()),
        LibSqlPermissionRepo::new(database.clone()),
        LibSqlDeviceRepo::new(database.clone()),
        key_service,
        app_config.bootstrap.clone(),
    );

    let device_identity = app_handle
        .try_state::<Arc<DeviceIdentity>>()
        .ok_or_else(|| io::Error::other("device identity is missing"))?;
    bootstrap_service
        .ensure_system_baseline(Some((
            device_identity.endpoint_id().to_string(),
            device_identity
                .endpoint_address()
                .map_err(io::Error::other)?,
        )))
        .await
        .map_err(io::Error::other)?;
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
    let app_config = if config_path.exists() {
        AppConfig::try_from(config_path.as_path())
            .map_err(|e| tauri::Error::Io(io::Error::other(e)))?
    } else {
        let mut default_config = AppConfig::default();
        default_config.bootstrap.is_master = true;
        default_config.bootstrap.web = false;
        default_config.bootstrap.desktop = true;
        default_config.bootstrap.device_name = "Native Application Device".to_owned();
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

    let app_config = Arc::new(app_config);
    app_handle.manage(app_config.clone());
    Ok(app_config)
}

#[tauri::command]
pub async fn get_localhost_server_base_url(app_handle: AppHandle<Wry>) -> String {
    localhost_server_base_url_for(&app_handle).await
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
    let context = TunnelContext::default();
    let local_peer = app_handle
        .try_state::<Arc<DeviceIdentity>>()
        .ok_or_else(|| tauri::Error::Io(io::Error::other("device identity is missing")))?
        .endpoint_id();
    let runtime = AppFileSystemRuntime::new(data_dir, local_peer, context.transport_factory())
        .map_err(tauri::Error::Io)?;
    app_handle.manage(context);
    app_handle.manage(Arc::new(runtime));
    Ok(())
}

pub async fn init_device_identity(
    app_handle: &AppHandle<Wry>,
    app_config: &AppConfig,
) -> tauri::Result<()> {
    let identity = open_device_identity(&app_config.oauth2.issuer)
        .await
        .map_err(|error| tauri::Error::Io(io::Error::other(error)))?;
    app_handle.manage(Arc::new(identity));
    Ok(())
}

pub fn init_tunnel_manager(
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
    let authorizer = LidpTunnelAuthorizer::new(Arc::clone(&state), control_plane.clone());
    let local_authorizer = authorizer.local();
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
    let listener = Arc::clone(&manager);
    tauri::async_runtime::spawn(async move {
        listener.listen().await;
    });
    let allowlist = Arc::new(allowlist);
    app_handle
        .try_state::<TunnelContext>()
        .ok_or_else(|| tauri::Error::Io(io::Error::other("tunnel context is missing")))?
        .set(
            (*manager).clone(),
            (*allowlist).clone(),
            local_authorizer,
            control_plane,
        );
    app_handle.manage(allowlist);
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
    start_unified_localhost_server(router, listener, &data_dir);

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
    config.bootstrap.lidp_url = format!("{base_url}/lidp");
    config.bootstrap.lidp_management_url = format!("{base_url}/lidp-management");
    Arc::new(config)
}

pub async fn close(app_handle: &AppHandle<Wry>) -> io::Result<()> {
    if let Some(manager) = app_handle.try_state::<Arc<DeviceTunnelManager>>() {
        manager.close().await;
    }
    if let Some(database) = app_handle.try_state::<Database>() {
        close_database(database.inner())
            .await
            .map_err(io::Error::other)?;
    }
    Ok(())
}
