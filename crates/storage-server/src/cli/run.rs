use api::serve;
use axum::Router;
use clap::Parser;
use cli::{CliArgs, CliServerCommand, shutdown_signal};
use env_logger::Env;
use std::{
    io,
    net::{IpAddr, SocketAddr},
    path::Path,
    sync::Arc,
    time::Duration,
};
use tokio::{select, spawn, time::sleep};
use tokio_util::sync::CancellationToken;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};

use crate::{
    AppConfig, IdpClient, ManagementClient, RouterState, StorageReplicationRuntime,
    resource_router, router::openapi_router,
};
use iroh::EndpointId;
use storage_service::{DatabaseRuntime, ScopedFileSystemRuntime};

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

    let mut router_state = if app_config.idp_api_base_uri.trim().is_empty() {
        RouterState::new(&app_config.api_public_uri)
    } else {
        let client_id = app_config
            .idp_oauth_client_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "STORAGE_IDP_OAUTH_CLIENT_ID is required when STORAGE_IDP_API_BASE_URI is configured",
                )
            })?;
        let client_secret = app_config
            .idp_oauth_client_secret
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "STORAGE_IDP_OAUTH_CLIENT_SECRET is required when STORAGE_IDP_API_BASE_URI is configured",
                )
            })?;
        let issuer = app_config
            .idp_issuer_uri
            .trim()
            .strip_suffix('/')
            .unwrap_or(app_config.idp_issuer_uri.trim());
        if issuer.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "STORAGE_IDP_ISSUER_URI is required when STORAGE_IDP_API_BASE_URI is configured",
            ));
        }
        let audience = app_config
            .idp_service_audience
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "STORAGE_IDP_SERVICE_AUDIENCE is required when STORAGE_IDP_API_BASE_URI is configured",
                )
            })?;
        let idp_client = IdpClient::new(
            &app_config.idp_api_base_uri,
            client_id,
            client_secret,
            issuer,
            audience,
        )
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        RouterState::new(&app_config.api_public_uri).with_idp_client(idp_client)
    };
    if !app_config.management_api_base_uri.trim().is_empty() {
        let client_id = app_config
            .management_oauth_client_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| io::Error::new(
                io::ErrorKind::InvalidInput,
                "STORAGE_MANAGEMENT_OAUTH_CLIENT_ID is required when STORAGE_MANAGEMENT_API_BASE_URI is configured",
            ))?;
        let client_secret = app_config
            .management_oauth_client_secret
            .as_deref()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| io::Error::new(
                io::ErrorKind::InvalidInput,
                "STORAGE_MANAGEMENT_OAUTH_CLIENT_SECRET is required when STORAGE_MANAGEMENT_API_BASE_URI is configured",
            ))?;
        let audience = app_config
            .management_service_audience
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| io::Error::new(
                io::ErrorKind::InvalidInput,
                "STORAGE_MANAGEMENT_SERVICE_AUDIENCE is required when STORAGE_MANAGEMENT_API_BASE_URI is configured",
            ))?;
        let idp_api_base_uri = app_config
            .idp_api_base_uri
            .trim()
            .strip_suffix('/')
            .unwrap_or(app_config.idp_api_base_uri.trim());
        if idp_api_base_uri.is_empty() || app_config.idp_issuer_uri.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "STORAGE_IDP_API_BASE_URI and STORAGE_IDP_ISSUER_URI are required when STORAGE_MANAGEMENT_API_BASE_URI is configured",
            ));
        }
        let management_client = ManagementClient::new(
            &app_config.management_api_base_uri,
            idp_api_base_uri,
            client_id,
            client_secret,
            &app_config.idp_issuer_uri,
            audience,
        )
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        router_state = router_state.with_management_client(management_client);
    }

    let storage_network = if let Some(idp_client) = router_state.idp_client.clone() {
        let network =
            Arc::new(crate::StoragePeerNetwork::open(Path::new(&app_config.data_dir)).await?);
        log::info!("Storage Iroh endpoint: {}", network.endpoint_id());
        if let Err(error) = network.refresh_approved_peers(&idp_client).await {
            log::warn!("failed to refresh Storage Iroh peer allowlist: {error}");
        }
        let refresh_network = Arc::clone(&network);
        let refresh_token = cancellation_token.clone();
        let peer_refresh = spawn(async move {
            refresh_network
                .run_peer_refresh(idp_client, refresh_token)
                .await;
        });
        Some((network, peer_refresh))
    } else {
        None
    };

    let database_runtime = Arc::new(
        DatabaseRuntime::new(Path::new(&app_config.data_dir).join("databases"))
            .map_err(io::Error::other)?,
    );
    let file_system_runtime = storage_network
        .as_ref()
        .map(|(network, _)| {
            ScopedFileSystemRuntime::<EndpointId>::new(
                Path::new(&app_config.data_dir).join("filesystems"),
                network.endpoint_id(),
            )
            .map(Arc::new)
        })
        .transpose()
        .map_err(io::Error::other)?;
    let protocol_runtime = if let Some((network, _)) = &storage_network {
        match (
            router_state.management_client.clone(),
            file_system_runtime.clone(),
        ) {
            (Some(management), Some(file_systems)) => {
                let runtime = StorageReplicationRuntime::start(
                    network.server().clone(),
                    management,
                    Arc::clone(&database_runtime),
                    file_systems,
                    cancellation_token.clone(),
                )?;
                let protocol_router = network.server().router(runtime.data_handler());
                Some((runtime, protocol_router))
            }
            _ => {
                log::warn!(
                    "Storage replication is disabled without Management and filesystem runtime configuration"
                );
                None
            }
        }
    } else {
        None
    };
    let resource_routes = resource_router(
        router_state.clone(),
        Some(Arc::clone(&database_runtime)),
        file_system_runtime,
    );
    let prefix = app_config.server.prefix();
    let resource_routes = if prefix.is_empty() {
        resource_routes
    } else {
        Router::new().nest(prefix, resource_routes)
    };
    let router = openapi_router(router_state, prefix)
        .split_for_parts()
        .0
        .merge(resource_routes)
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
    if let Some((protocol_runtime, protocol_router)) = protocol_runtime {
        if let Err(error) = protocol_runtime.shutdown().await {
            log::warn!("Storage replication runtime shutdown failed: {error}");
        }
        drop(protocol_router);
    }
    if let Some((network, peer_refresh)) = storage_network {
        if let Err(error) = peer_refresh.await {
            log::warn!("Storage Iroh peer refresh task failed: {error}");
        }
        network.close().await;
    }

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

    Ok(())
}
