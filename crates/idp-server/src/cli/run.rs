use std::{
    io,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use api::serve;
use clap::Parser;
use cli::{CliArgs, CliServerCommand, shutdown_signal};
use db::open_native_engine;
use env_logger::Env;
use idp_model::contract::DeviceState;
use idp_service::{
    oauth2::OAuth2Service,
    replica::{
        DbApplicationRepo, DbClientRepo, DbKeyRepo, DbOAuth2AuthorizationCodeRepo,
        DbOAuth2UserConsentRepo, DbUserRepo,
    },
    repo::{KeyService, PrivateKeyKeyringRepo},
};
use iroh_chain::EndpointIdStore;
use management_server::{
    RouterState as ManagementRouterState, openapi_router as management_router,
};
use management_service::{
    DeviceRepo, HostedControlPlane, ManagementService,
    replica::{DbDeviceRepo, DbPermissionRepo, DbRoleRepo, DbSelectionPolicyRepo},
};
use storage_service::{DatabaseRuntime, ScopedFileSystemRuntime};
use tokio::{select, spawn, time::sleep};
use tokio_util::sync::CancellationToken;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};

use crate::{
    AppConfig, RouterState, TimedPairingAcceptanceController, router::openapi_router,
    storage_router,
};

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

    std::fs::create_dir_all(&app_config.data_dir)?;
    let engine = Arc::new(
        open_native_engine(Path::new(&app_config.data_dir).join("idp.redb"))
            .map_err(io::Error::other)?,
    );
    idp_model::replica::up(&engine)
        .await
        .map_err(io::Error::other)?;

    let key_service = Arc::new(KeyService::new(
        DbKeyRepo::new(Arc::clone(&engine)),
        PrivateKeyKeyringRepo::new(&app_config.oauth2.issuer),
        app_config.key_namespace.clone(),
    ));
    let devices = Arc::new(DbDeviceRepo::new(Arc::clone(&engine)));
    let allowed_peers = EndpointIdStore::default();
    allowed_peers.replace(
        devices
            .list()
            .await
            .map_err(io::Error::other)?
            .into_iter()
            .filter(|device| device.state == DeviceState::Approved)
            .filter_map(|device| device.public_key.parse().ok()),
    );
    let (device_identity, manager) =
        crate::open_device_identity_with_allowlist(allowed_peers.clone()).await?;
    let device_identity = Arc::new(device_identity);
    let oauth2_service = Arc::new(OAuth2Service::new(
        DbApplicationRepo::new(Arc::clone(&engine)),
        DbClientRepo::new(Arc::clone(&engine), Arc::clone(&key_service)),
        DbOAuth2AuthorizationCodeRepo::new(Arc::clone(&engine)),
        DbUserRepo::new(Arc::clone(&engine), app_config.password.clone()),
        DbOAuth2UserConsentRepo::new(Arc::clone(&engine)),
        key_service,
        app_config.oauth2.clone(),
    ));

    let control_plane = app_config
        .control_plane_uri
        .as_deref()
        .map(HostedControlPlane::new)
        .transpose()
        .map_err(io::Error::other)?
        .map(Arc::new);
    let management_control_plane = Arc::new(
        HostedControlPlane::new_with_issuer(&app_config.api_public_uri, &app_config.oauth2.issuer)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?,
    );
    let storage_audience = app_config
        .storage_audience
        .as_deref()
        .unwrap_or(&app_config.api_public_uri);
    if storage_audience.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "storage_audience must not be empty",
        ));
    }
    let management_state = ManagementRouterState::new(
        &app_config.api_public_uri,
        Arc::new(ManagementService::new(
            DbApplicationRepo::new(Arc::clone(&engine)),
            DbPermissionRepo::new(Arc::clone(&engine)),
            DbRoleRepo::new(Arc::clone(&engine)),
        )),
        Arc::clone(&oauth2_service),
        Arc::new(DbSelectionPolicyRepo::new(Arc::clone(&engine))),
        management_control_plane,
        storage_audience,
    )
    .with_devices(Arc::clone(&devices));
    let router_state = RouterState::new(
        &app_config.ui_public_uri,
        &app_config.api_public_uri,
        Arc::clone(&engine),
        Arc::clone(&oauth2_service),
        Arc::clone(&devices),
        Arc::clone(&device_identity),
    );
    let router_state = match &control_plane {
        Some(control_plane) => router_state.with_hosted_control_plane(Arc::clone(control_plane)),
        None => router_state,
    };

    let storage_root = PathBuf::from(&args.config)
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let databases = Arc::new(DatabaseRuntime::new(storage_root.clone()).map_err(io::Error::other)?);
    let file_systems = Arc::new(
        ScopedFileSystemRuntime::new(storage_root, device_identity.endpoint_id())
            .map_err(io::Error::other)?,
    );
    let router_state = router_state
        .with_storage_databases(Arc::clone(&databases))
        .with_storage_file_systems(Arc::clone(&file_systems));
    router_state
        .bootstrap_grants
        .set_admission_server(manager.clone(), crate::bootstrap::BOOTSTRAP_ALPN);
    let refresh_store = allowed_peers;
    let refresh_devices = Arc::clone(&devices);
    let peer_refresh = spawn(async move {
        loop {
            sleep(Duration::from_secs(2)).await;
            match refresh_devices.list().await {
                Ok(devices) => refresh_store.replace(
                    devices
                        .into_iter()
                        .filter(|device| device.state == DeviceState::Approved)
                        .filter_map(|device| device.public_key.parse().ok()),
                ),
                Err(error) => log::warn!("failed to refresh Iroh allowlist: {error}"),
            }
        }
    });
    router_state
        .pairing_acceptance
        .bind(Arc::new(TimedPairingAcceptanceController::new(
            manager.clone(),
            Duration::from_secs(app_config.pairing.accepting_timeout_seconds),
        )))
        .map_err(io::Error::other)?;
    let policies = Arc::new(DbSelectionPolicyRepo::new(Arc::clone(&engine)));
    let database_protocol = crate::database_protocol::DatabaseProtocolHandler::new(
        Arc::clone(&device_identity),
        Arc::clone(&devices),
        Arc::clone(&policies),
        databases,
    );
    let storage_protocol = crate::storage_protocol::StorageProtocolHandler::new(
        Arc::clone(&device_identity),
        Arc::clone(&devices),
        Arc::clone(&policies),
        Arc::clone(&file_systems),
    );
    let bootstrap_protocol = crate::bootstrap::BootstrapProtocolHandler::new(
        Arc::clone(&engine),
        Arc::clone(&router_state.bootstrap_grants),
        Arc::clone(&router_state.devices),
    );
    let data_protocol = crate::data_protocol::DataProtocolHandler::new(
        database_protocol.clone(),
        storage_protocol.clone(),
    );
    let _iroh_router = manager.router_with_protocol(
        data_protocol,
        crate::bootstrap::BOOTSTRAP_ALPN,
        bootstrap_protocol,
    );
    let filesystem_sync_manager = manager.clone();
    let filesystem_protocol = storage_protocol.clone();
    let filesystem_sync = spawn(async move {
        loop {
            filesystem_protocol
                .synchronize_selected_peers(&filesystem_sync_manager)
                .await;
            sleep(Duration::from_secs(10)).await;
        }
    });
    let sync_manager = manager.clone();
    let database_sync = spawn(async move {
        loop {
            database_protocol
                .synchronize_selected_peers(&sync_manager)
                .await;
            sleep(Duration::from_secs(10)).await;
        }
    });
    log::info!("Iroh endpoint: {:?}", device_identity.endpoint().addr());

    let router = openapi_router(router_state.clone(), app_config.server.prefix())
        .split_for_parts()
        .0
        .merge(storage_router(router_state, file_systems))
        .merge(
            management_router(management_state, "/management")
                .split_for_parts()
                .0,
        )
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
    peer_refresh.abort();
    filesystem_sync.abort();
    database_sync.abort();
    let mut command_handle = command_handle;
    select! {
        result = &mut command_handle => match result {
            Ok(Ok(())) => log::info!("server shutdown complete"),
            Ok(Err(error)) => log::error!("command error: {error}"),
            Err(error) => log::error!("join error: {error}"),
        },
        _ = sleep(Duration::from_secs(10)) => {
            log::warn!("server shutdown timed out, aborting serve task");
            command_handle.abort();
        }
    }

    Ok(())
}
