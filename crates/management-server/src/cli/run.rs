use std::{
    fs::create_dir_all,
    io,
    net::{IpAddr, SocketAddr},
    path::Path,
    sync::Arc,
    time::Duration,
};

use api::serve;
use clap::Parser;
use cli::{CliArgs, CliServerCommand, shutdown_signal};
use db::open_native_engine;
use env_logger::Env;
use idp_service::{
    oauth2::OAuth2Service,
    replica::{
        DbApplicationRepo, DbClientRepo, DbKeyRepo, DbOAuth2AuthorizationCodeRepo,
        DbOAuth2UserConsentRepo, DbUserRepo,
    },
    repo::{KeyService, PrivateKeyKeyringRepo},
};
use management_service::{
    ManagementService,
    replica::{DbPermissionRepo, DbRoleRepo},
};
use tokio::{select, spawn, time::sleep};
use tokio_util::sync::CancellationToken;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};

use crate::{AppConfig, RouterState, router::openapi_router};

pub async fn run() -> io::Result<()> {
    match dotenvy::dotenv() {
        Ok(_) => {}
        Err(error) => eprintln!("failed to load .env file: {error}"),
    }

    let args = CliArgs::parse();
    let cancellation_token = CancellationToken::new();
    let app_config = Arc::new(match AppConfig::try_from(Path::new(&args.config)) {
        Ok(app_config) => app_config,
        Err(error) => {
            eprintln!("failed to load config {:?}: {error}", args.config);
            AppConfig::default()
        }
    });

    env_logger::Builder::from_env(Env::default().default_filter_or(&app_config.log_level)).init();

    create_dir_all(&app_config.data_dir)?;
    let engine = Arc::new(
        open_native_engine(Path::new(&app_config.data_dir).join("management.redb"))
            .map_err(io::Error::other)?,
    );
    idp_model::replica::up(&engine)
        .await
        .map_err(io::Error::other)?;

    let key_service = Arc::new(KeyService::new(
        DbKeyRepo::new(Arc::clone(&engine)),
        PrivateKeyKeyringRepo::new(&app_config.oauth2.issuer),
        &app_config.key_namespace,
    ));
    let oauth2_service = Arc::new(OAuth2Service::new(
        DbApplicationRepo::new(Arc::clone(&engine)),
        DbClientRepo::new(Arc::clone(&engine), Arc::clone(&key_service)),
        DbOAuth2AuthorizationCodeRepo::new(Arc::clone(&engine)),
        DbUserRepo::new(Arc::clone(&engine)),
        DbOAuth2UserConsentRepo::new(Arc::clone(&engine)),
        key_service,
        app_config.oauth2.clone(),
    ));
    let management_service = Arc::new(ManagementService::new(
        DbApplicationRepo::new(Arc::clone(&engine)),
        DbPermissionRepo::new(Arc::clone(&engine)),
        DbRoleRepo::new(Arc::clone(&engine)),
    ));
    let router_state = RouterState::new(
        &app_config.api_public_uri,
        management_service,
        oauth2_service,
    );
    let router = openapi_router(router_state, app_config.server.prefix())
        .layer(CorsLayer::very_permissive().allow_private_network(true))
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new().gzip(app_config.server.gzip))
        .into();

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
        result = &mut command_handle => match result {
            Ok(Ok(())) => log::info!("server shutdown complete"),
            Ok(Err(error)) => log::error!("command error: {error}"),
            Err(error) => log::error!("join error: {error}"),
        },
        _ = sleep(shutdown_timeout) => {
            log::warn!("server shutdown timed out after {shutdown_timeout:?}, aborting serve task");
            command_handle.abort();
            sleep(Duration::from_millis(100)).await;
        }
    }

    Ok(())
}
