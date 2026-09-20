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
use idp_service::{
    oauth2::OAuth2Service,
    replica::{
        DbApplicationRepo, DbClientRepo, DbKeyRepo, DbOAuth2AuthorizationCodeRepo,
        DbOAuth2UserConsentRepo, DbUserRepo,
    },
    repo::{KeyService, PrivateKeyKeyringRepo},
};
use iroh::EndpointId;
use iroh_chain::{DynamicEndpointIdStore, Server, TunnelAuthorizer, VaultId};
use management_service::{
    HostedControlPlane, access_token_authorization::HostedAccessTokenAuthorizer,
    replica::DbDeviceRepo,
};
use storage_service::ScopedFileSystemRuntime;
use tokio::{select, spawn, time::sleep};
use tokio_util::sync::CancellationToken;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};

use crate::{
    AppConfig, RouterState, TimedPairingAcceptanceController, router::openapi_router,
    storage_router,
};

enum CliTunnelAuthorizer {
    Hosted(HostedAccessTokenAuthorizer),
    Deny,
}

impl TunnelAuthorizer for CliTunnelAuthorizer {
    async fn authorize(
        &self,
        vault_id: VaultId,
        initiating_id: EndpointId,
        accepting_id: EndpointId,
        authorization: &[u8],
    ) -> bool {
        match self {
            Self::Hosted(authorizer) => {
                authorizer
                    .authorize(vault_id, initiating_id, accepting_id, authorization)
                    .await
            }
            Self::Deny => false,
        }
    }
}

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
    let device_identity = Arc::new(crate::open_device_identity().await?);
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
    let file_systems = Arc::new(
        ScopedFileSystemRuntime::new(storage_root, device_identity.endpoint_id())
            .map_err(io::Error::other)?,
    );
    let router_state = router_state.with_storage_file_systems(Arc::clone(&file_systems));
    let authorizer = match control_plane {
        Some(control_plane) => {
            CliTunnelAuthorizer::Hosted(HostedAccessTokenAuthorizer::new(control_plane))
        }
        None => CliTunnelAuthorizer::Deny,
    };
    let manager = Server::new(
        device_identity.endpoint(),
        DynamicEndpointIdStore::default(),
        authorizer,
    );
    router_state
        .pairing_acceptance
        .bind(Arc::new(TimedPairingAcceptanceController::new(
            manager.clone(),
            Duration::from_secs(app_config.pairing.accepting_timeout_seconds),
        )))
        .map_err(io::Error::other)?;
    let listener = manager.clone();
    spawn(async move { listener.listen().await });
    log::info!("Iroh endpoint: {:?}", device_identity.endpoint().addr());

    let router = openapi_router(router_state.clone(), app_config.server.prefix())
        .split_for_parts()
        .0
        .merge(storage_router(router_state, file_systems))
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
