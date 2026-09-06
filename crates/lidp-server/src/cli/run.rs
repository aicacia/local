use api::serve;
use clap::Parser;
use cli::{CliArgs, CliServerCommand, shutdown_signal};
use db::{close_database, open_database};
use env_logger::Env;
use lidp_service::{
    bootstrap::BootstrapService,
    oauth2::OAuth2Service,
    repo::{
        KeyService, LibSqlApplicationRepo, LibSqlClientRepo, LibSqlKeyRepo,
        LibSqlOAuth2AuthorizationCodeRepo, LibSqlOAuth2UserConsentRepo, LibSqlPermissionRepo,
        LibSqlRoleRepo, LibSqlUserDeviceRepo, LibSqlUserRepo, PrivateKeyKeyringRepo,
    },
    scoped_file_system::ScopedFileSystemRuntime,
    storage_session::StorageSessionService,
};
use std::{
    io,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{select, spawn, time::sleep};
use tokio_util::sync::CancellationToken;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};

use crate::{AppConfig, RouterState, router::openapi_router, storage_router};

pub async fn run() -> io::Result<()> {
    match dotenvy::dotenv() {
        Ok(_) => {}
        Err(e) => {
            eprintln!("failed to load .env file: {}", e);
        }
    }

    let args = CliArgs::parse();

    let cancellation_token = CancellationToken::new();

    let app_config = Arc::new(match AppConfig::try_from(Path::new(&args.config)) {
        Ok(app_config) => app_config,
        Err(e) => {
            eprintln!("failed to load config {:?}: {}", args.config, e);
            AppConfig::default()
        }
    });

    env_logger::Builder::from_env(Env::default().default_filter_or(&app_config.log_level)).init();

    let database = open_database(&app_config.database).await.map_err(|e| {
        log::error!("failed to create database pool: {}", e);
        io::Error::other(e)
    })?;

    lidp_model::migrate::up(&database).await.map_err(|e| {
        log::error!("failed to run database migrations: {}", e);
        io::Error::other(e)
    })?;

    let database = Arc::new(database);
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
        key_service.clone(),
        app_config.bootstrap.clone(),
    );

    bootstrap_service
        .ensure_system_baseline()
        .await
        .map_err(io::Error::other)?;

    let oauth2_config = app_config.oauth2.clone();
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
        oauth2_config,
    ));
    let storage_sessions = Arc::new(StorageSessionService::new());
    let storage_root = PathBuf::from(&args.config)
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let file_systems = Arc::new(
        ScopedFileSystemRuntime::new(storage_root, &app_config.oauth2.issuer)
            .map_err(io::Error::other)?,
    );
    let router_state = RouterState::new(
        &app_config.ui_public_uri,
        &app_config.api_public_uri,
        database.clone(),
        oauth2_service,
        storage_sessions.clone(),
        Arc::new(LibSqlUserDeviceRepo::new(database.clone())),
    );

    let router = openapi_router(router_state, app_config.server.prefix())
        .split_for_parts()
        .0
        .merge(storage_router(storage_sessions, file_systems))
        .layer(CorsLayer::very_permissive().allow_private_network(true))
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new().gzip(app_config.server.gzip));

    let run_serve = |host: Option<IpAddr>, port: Option<u16>| {
        let addr = SocketAddr::from((
            host.unwrap_or(app_config.server.host),
            port.unwrap_or(app_config.server.port),
        ));

        spawn(serve(router, addr, cancellation_token.clone()))
    };

    let command_handle = match args.command {
        #[cfg(feature = "completions")]
        Some(CliServerCommand::Completions { shell }) => {
            spawn(async move { cli::run_completions(shell).await })
        }
        Some(CliServerCommand::Serve { serve }) => run_serve(serve.host, serve.port),
        None => run_serve(None, None),
    };

    shutdown_signal(cancellation_token).await;

    let shutdown_timeout = Duration::from_secs(10);
    let mut command_handle = command_handle;
    select! {
      res = &mut command_handle => {
        match res {
          Ok(Ok(_)) => log::info!("server shutdown complete"),
          Ok(Err(e)) => log::error!("command error: {}", e),
          Err(e) => log::error!("join error: {}", e),
        }
      }
      _ = sleep(shutdown_timeout) => {
        log::warn!("server shutdown timed out after {:?}, aborting serve task", shutdown_timeout);
        command_handle.abort();
        sleep(Duration::from_millis(100)).await;
      }
    }

    close_database(&database).await.map_err(|e| {
        log::error!("failed to close database pool: {}", e);
        io::Error::other(e)
    })?;

    Ok(())
}
