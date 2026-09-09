use api::serve;
use clap::Parser;
use cli::{CliArgs, CliServerCommand, shutdown_signal};
use db::{close_database, open_database};
use env_logger::Env;
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey, endpoint::presets};
use iroh_chain::{DynamicEndpointIdStore, Server, TUNNEL_ALPN, TunnelAuthorizer, VaultId};
use lidp_service::{
    bootstrap::BootstrapService,
    hosted_control_plane::HostedControlPlane,
    oauth2::OAuth2Service,
    repo::{
        KeyService, LibSqlApplicationRepo, LibSqlClientRepo, LibSqlKeyRepo,
        LibSqlOAuth2AuthorizationCodeRepo, LibSqlOAuth2UserConsentRepo, LibSqlPermissionRepo,
        LibSqlRoleRepo, LibSqlUserDeviceRepo, LibSqlUserRepo, PrivateKeyKeyringRepo,
    },
    storage_session::{StorageScope, StorageSessionService},
    tunnel_authorization::{
        HostedTunnelAuthorizationProvider, HostedTunnelAuthorizer,
        LocalTunnelAuthorizationProvider, LocalTunnelAuthorizer,
    },
};
use std::{
    future::Future,
    io::{self, Error},
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::Duration,
};
use storage_service::{
    IrohTransportFactory, ScopedFileSystemRuntime, ScopedTunnelAuthorizationProvider,
    TrustedEndpointAddrLookup, TunnelAuthorizationProvider,
};
use tokio::{select, spawn, time::sleep};
use tokio_util::sync::CancellationToken;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};

use crate::{
    AppConfig, RouterState,
    router::{HostedStorageScopeResolver, openapi_router},
    storage_router,
};

enum CliTunnelAuthorizer {
    Hosted(HostedTunnelAuthorizer),
    Local(LocalTunnelAuthorizer),
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
            Self::Local(authorizer) => {
                authorizer
                    .authorize(vault_id, initiating_id, accepting_id, authorization)
                    .await
            }
        }
    }
}

#[derive(Clone)]
enum CliAuthorizationProvider {
    Hosted(Arc<HostedControlPlane>),
    Local(Arc<LocalTunnelAuthorizer>),
}

impl ScopedTunnelAuthorizationProvider<StorageScope> for CliAuthorizationProvider {
    type Authorization = CliTunnelAuthorization;

    fn authorization(&self, scope: &StorageScope) -> Result<Self::Authorization, String> {
        Ok(match self {
            Self::Hosted(control_plane) => CliTunnelAuthorization::Hosted(
                HostedTunnelAuthorizationProvider::new(Arc::clone(control_plane), scope.clone()),
            ),
            Self::Local(authorizer) => {
                CliTunnelAuthorization::Local(authorizer.authorization_provider(scope.clone()))
            }
        })
    }
}

#[derive(Clone)]
enum CliTunnelAuthorization {
    Hosted(HostedTunnelAuthorizationProvider),
    Local(LocalTunnelAuthorizationProvider),
}

impl TunnelAuthorizationProvider for CliTunnelAuthorization {
    fn authorization(
        &self,
        vault_id: VaultId,
        local_id: EndpointId,
        remote_id: EndpointId,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, Error>> + Send + '_>> {
        match self {
            Self::Hosted(provider) => provider.authorization(vault_id, local_id, remote_id),
            Self::Local(provider) => provider.authorization(vault_id, local_id, remote_id),
        }
    }
}

#[derive(Clone)]
struct ScopeTrustedEndpoints {
    control_plane: Option<Arc<HostedControlPlane>>,
}

impl TrustedEndpointAddrLookup<StorageScope> for ScopeTrustedEndpoints {
    async fn trusted_endpoint_addrs(
        &self,
        scope: &StorageScope,
    ) -> Result<Vec<EndpointAddr>, String> {
        let trusted_devices = match &self.control_plane {
            Some(control_plane) => control_plane.trusted_devices(&scope.access_token).await?,
            None => scope.trusted_devices.clone(),
        };
        Ok(trusted_devices
            .iter()
            .filter_map(|device| serde_json::from_str(&device.address).ok())
            .collect())
    }
}

async fn open_device_identity(key_path: &Path) -> io::Result<crate::DeviceIdentity> {
    let secret_key = match std::fs::read(&key_path) {
        Ok(bytes) => SecretKey::from_bytes(
            &bytes
                .try_into()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid Iroh key"))?,
        ),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let key = SecretKey::generate();
            std::fs::write(&key_path, key.to_bytes())?;
            key
        }
        Err(error) => return Err(error),
    };
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key.clone())
        .alpns(vec![TUNNEL_ALPN.to_vec()])
        .bind()
        .await
        .map_err(io::Error::other)?;
    Ok(crate::DeviceIdentity::new(endpoint, secret_key))
}

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
    let user_devices = Arc::new(LibSqlUserDeviceRepo::new(database.clone()));
    let control_plane = app_config
        .control_plane_uri
        .as_deref()
        .map(HostedControlPlane::new)
        .transpose()
        .map_err(io::Error::other)?
        .map(Arc::new);
    let device_identity = Arc::new(open_device_identity(Path::new(&app_config.device_key)).await?);
    let router_state = RouterState::new(
        &app_config.ui_public_uri,
        &app_config.api_public_uri,
        database.clone(),
        Arc::clone(&oauth2_service),
        storage_sessions.clone(),
        Arc::clone(&user_devices),
        Arc::clone(&device_identity),
    );
    let router_state = match &control_plane {
        Some(control_plane) => router_state.with_storage_scope_resolver(Arc::new(
            HostedStorageScopeResolver::new(Arc::clone(control_plane)),
        )),
        None => router_state,
    };
    let storage_root = PathBuf::from(&args.config)
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let allowlist = DynamicEndpointIdStore::default();
    let authorizer = match &control_plane {
        Some(control_plane) => {
            CliTunnelAuthorizer::Hosted(HostedTunnelAuthorizer::new(Arc::clone(control_plane)))
        }
        None => CliTunnelAuthorizer::Local(LocalTunnelAuthorizer::new(
            Arc::clone(&oauth2_service),
            Arc::clone(&user_devices),
        )),
    };
    let manager = Server::new(device_identity.endpoint(), allowlist.clone(), authorizer);
    let listener = manager.clone();
    spawn(async move {
        listener.listen().await;
    });
    log::info!("Iroh endpoint: {:?}", device_identity.endpoint().addr());
    let authorization_provider = match &control_plane {
        Some(control_plane) => CliAuthorizationProvider::Hosted(Arc::clone(control_plane)),
        None => CliAuthorizationProvider::Local(Arc::new(LocalTunnelAuthorizer::new(
            Arc::clone(&oauth2_service),
            Arc::clone(&user_devices),
        ))),
    };
    let transport_factory = IrohTransportFactory::new(
        manager,
        allowlist,
        authorization_provider,
        ScopeTrustedEndpoints { control_plane },
    );
    let file_systems = Arc::new(
        ScopedFileSystemRuntime::new(
            storage_root,
            device_identity.endpoint_id(),
            transport_factory,
        )
        .map_err(io::Error::other)?,
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
