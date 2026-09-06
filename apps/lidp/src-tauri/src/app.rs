use std::{fs, io, path::Path, sync::Arc};

use axum::Router;
use db::{close_database, open_database};
use libsql::Database;
use lidp_server::{AppConfig, RouterState, storage_router};
use lidp_service::{
    bootstrap::BootstrapService,
    management::ManagementService,
    oauth2::OAuth2Service,
    repo::{
        KeyService, LibSqlApplicationRepo, LibSqlClientRepo, LibSqlKeyRepo,
        LibSqlOAuth2AuthorizationCodeRepo, LibSqlOAuth2UserConsentRepo, LibSqlPermissionRepo,
        LibSqlRoleRepo, LibSqlUserDeviceRepo, LibSqlUserRepo, PrivateKeyKeyringRepo,
    },
    scoped_file_system::ScopedFileSystemRuntime,
    storage_session::StorageSessionService,
};
use tauri::{AppHandle, Manager, Wry, async_runtime::Mutex};
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;

use crate::device_identity::DeviceIdentity;
use crate::localhost_server::{
    localhost_server_base_url, reserve_localhost_listener, start_unified_localhost_server,
};

#[derive(Clone, Default)]
pub struct LocalhostServerState {
    pub base_url: String,
    pub ready: bool,
}

pub fn init_router(
    app_config: Arc<AppConfig>,
    database: Arc<Database>,
    file_systems: Arc<ScopedFileSystemRuntime>,
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
    let router_state = Arc::new(RouterState::new(
        &app_config.ui_public_uri,
        &app_config.api_public_uri,
        database.clone(),
        oauth2_service.clone(),
        storage_sessions.clone(),
        Arc::new(LibSqlUserDeviceRepo::new(database.clone())),
    ));
    let lidp_router = lidp_server::openapi_router(router_state.as_ref().clone(), "");
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
            .layer(CorsLayer::very_permissive().allow_private_network(true)),
        router_state,
    ))
}

pub async fn init_datebase(
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
        key_service,
        app_config.bootstrap.clone(),
    );

    bootstrap_service
        .ensure_system_baseline()
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

#[tauri::command]
pub fn get_device_endpoint_id(app_handle: AppHandle<Wry>) -> Result<String, String> {
    app_handle
        .try_state::<Arc<DeviceIdentity>>()
        .map(|identity| identity.endpoint_id().to_string())
        .ok_or_else(|| "device identity is missing".to_owned())
}

#[tauri::command]
pub fn get_device_endpoint_address(app_handle: AppHandle<Wry>) -> Result<String, String> {
    app_handle
        .try_state::<Arc<DeviceIdentity>>()
        .ok_or_else(|| "device identity is missing".to_owned())?
        .endpoint_address()
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
    app_config: &AppConfig,
) -> tauri::Result<()> {
    let data_dir = app_handle.path().app_data_dir()?;
    let runtime = ScopedFileSystemRuntime::new(data_dir, &app_config.oauth2.issuer)
        .map_err(tauri::Error::Io)?;
    app_handle.manage(Arc::new(runtime));
    Ok(())
}

pub async fn init_device_identity(
    app_handle: &AppHandle<Wry>,
    app_config: &AppConfig,
) -> tauri::Result<()> {
    let identity = DeviceIdentity::open(&app_config.oauth2.issuer)
        .await
        .map_err(|error| tauri::Error::Io(io::Error::other(error)))?;
    app_handle.manage(Arc::new(identity));
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
    config.oauth2.issuer = base_url.to_owned();
    config.ui_public_uri = base_url.to_owned();
    config.api_public_uri = base_url.to_owned();
    config.bootstrap.lidp_url = base_url.to_owned();
    config.bootstrap.lidp_management_url = format!("{base_url}/lidp-management");
    Arc::new(config)
}

pub async fn close(app_handle: &AppHandle<Wry>) -> io::Result<()> {
    if let Some(database) = app_handle.try_state::<Database>() {
        close_database(database.inner())
            .await
            .map_err(io::Error::other)?;
    }
    Ok(())
}
