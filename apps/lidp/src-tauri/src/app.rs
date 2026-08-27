use std::{fs, io, path::Path, sync::Arc, time::Duration};

use axum::{Router, http::StatusCode, response::IntoResponse};
use db::{close_database, open_database};
use libsql::Database;
use lidp_server::{AppConfig, RouterState};
use lidp_service::{
    bootstrap::BootstrapService,
    management::ManagementService,
    oauth2::OAuth2Service,
    repo::{
        KeyService, LibSqlApplicationRepo, LibSqlClientRepo, LibSqlKeyRepo,
        LibSqlOAuth2AuthorizationCodeRepo, LibSqlOAuth2UserConsentRepo, LibSqlPermissionRepo,
        LibSqlRoleRepo, LibSqlUserRepo, PrivateKeyKeyringRepo,
    },
};
use tauri::{AppHandle, Manager, async_runtime::Mutex};
use tauri_plugin_fetch_api::{Request, Response};
use tauri_plugin_opener::OpenerExt;
use tower_service::Service;

use crate::bridge::{
    StorageBridge, bridge_trust_prompted_path, bridge_trust_url, ensure_storage_bridge_certificate,
};

pub fn init_router(app_config: Arc<AppConfig>, database: Arc<Database>) -> io::Result<Router> {
    let key_service = Arc::new(KeyService::new(
        LibSqlKeyRepo::new(database.clone()),
        PrivateKeyKeyringRepo::new(&app_config.oauth2.issuer),
        app_config.key_namespace.clone(),
    ));

    let oauth2_service = Arc::new(OAuth2Service::new(
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
        app_config.key_namespace.clone(),
    ));

    let lidp_router = lidp_server::openapi_router(
        RouterState::new(
            "lidp://app",
            "lidp://app",
            database.clone(),
            oauth2_service.clone(),
        ),
        "",
    );
    let management_service = Arc::new(ManagementService::new(
        LibSqlApplicationRepo::new(database.clone()),
        LibSqlPermissionRepo::new(database.clone()),
        LibSqlRoleRepo::new(database.clone()),
    ));
    let management_router = lidp_management_server::openapi_router(
        lidp_management_server::RouterState::new(
            "lidp://app",
            database,
            management_service,
            oauth2_service,
        ),
        "/lidp-management",
    );

    Ok(lidp_router
        .split_for_parts()
        .0
        .merge(management_router.split_for_parts().0))
}

pub async fn init_datebase(
    app_handle: AppHandle,
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
    app_handle: &AppHandle,
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
        default_config.oauth2.issuer = "lidp://app".to_owned();
        default_config.ui_public_uri = "lidp://app".to_owned();
        default_config.api_public_uri = "lidp://app".to_owned();
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

pub async fn request_handler(app_handle: AppHandle, request: Request) -> Response {
    let router_state = app_handle.state::<Mutex<Router>>();
    let mut router = router_state.lock().await;

    log::debug!("handling request: {:?}", request);

    match router.call(request).await {
        Ok(response) => {
            log::debug!("request handled successfully: {:?}", response);
            response
        }
        Err(err) => {
            log::error!("error handling request: {err}");
            let mut response = format!("Internal Server Error: {err}").into_response();
            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            response
        }
    }
}

async fn bridge_url_for(app_handle: &AppHandle) -> String {
    if let Some(bridge_state) = app_handle.try_state::<Mutex<StorageBridge>>() {
        bridge_state.lock().await.url().await
    } else {
        String::new()
    }
}

#[tauri::command]
pub async fn get_storage_bridge_url(app_handle: AppHandle) -> String {
    bridge_url_for(&app_handle).await
}

#[tauri::command]
pub async fn open_storage_bridge_trust_page(app_handle: AppHandle) -> Result<(), String> {
    let trust_url = bridge_trust_url(&bridge_url_for(&app_handle).await)
        .ok_or("storage bridge URL is not available yet")?;
    app_handle
        .opener()
        .open_url(trust_url, None::<&str>)
        .map_err(|err| err.to_string())
}

async fn prompt_bridge_cert_trust_if_needed(app_handle: AppHandle, data_dir: std::path::PathBuf) {
    let prompted_path = bridge_trust_prompted_path(&data_dir);
    for _ in 0..50 {
        let wss_url = bridge_url_for(&app_handle).await;
        if wss_url.is_empty() {
            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        }
        if prompted_path.exists() {
            return;
        }
        let Some(trust_url) = bridge_trust_url(&wss_url) else {
            return;
        };
        if app_handle
            .opener()
            .open_url(trust_url, None::<&str>)
            .is_err()
        {
            return;
        }
        let _ = fs::write(prompted_path, b"");
        return;
    }
}

pub async fn init_storage_bridge(app_handle: &AppHandle) -> tauri::Result<()> {
    let data_dir = app_handle.path().app_data_dir()?;
    ensure_storage_bridge_certificate(&data_dir)
        .await
        .map_err(|err| tauri::Error::Io(io::Error::other(err)))?;
    let files_dir = data_dir.join("files");
    if !files_dir.exists() {
        fs::create_dir_all(&files_dir)?;
    }

    let bridge = StorageBridge::new(files_dir).await;
    let server_bridge = bridge.clone();
    let bridge_data_dir = data_dir.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(err) = server_bridge.start_server(&bridge_data_dir).await {
            log::error!("storage websocket bridge failed to start: {err}");
        }
    });
    app_handle.manage(Mutex::new(bridge));

    let prompt_handle = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        prompt_bridge_cert_trust_if_needed(prompt_handle, data_dir).await;
    });
    Ok(())
}

pub async fn close(app_handle: &AppHandle) -> io::Result<()> {
    if let Some(database) = app_handle.try_state::<Database>() {
        close_database(database.inner())
            .await
            .map_err(io::Error::other)?;
    }
    Ok(())
}
