use api::serve;

use bootstrap_service::bootstrap::{BootstrapConfig, BootstrapInput, BootstrapService};
use clap::Parser;
use cli::{CliArgs, CliServerCommand, shutdown_signal};
use db::{close_database, open_database};
use env_logger::Env;
use idp_service::libsql::{
    LibSqlApplicationRepo, LibSqlClientRepo, LibSqlKeyRepo, LibSqlOAuth2AuthorizationCodeRepo,
    LibSqlOAuth2UserConsentRepo, LibSqlUserRepo,
};
use idp_service::{
    generate_random_string,
    oauth2::OAuth2Service,
    repo::{KeyService, PrivateKeyKeyringRepo},
};
use iroh::{EndpointAddr, EndpointId};
use iroh_chain::{DynamicEndpointIdStore, Server, TunnelAuthorizer, VaultId};
use iroh_chain_file_system::{EndpointIdCodec, ScopedIrohTransport, StaticTunnelAuthorization};
use management_service::{
    HostedControlPlane, StorageScope, StorageSessionService,
    libsql::{LibSqlDeviceRepo, LibSqlPermissionRepo, LibSqlRoleRepo},
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
    GlobalIdentityRuntime, IrohTransportFactory, ScopedFileSystemRuntime,
    ScopedTunnelAuthorizationProvider, TrustedEndpointAddrLookup, TunnelAuthorizationProvider,
};
use tokio::{select, spawn, time::sleep};
use tokio_util::sync::CancellationToken;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};

use crate::{
    ActiveGlobalIdentityReadGate, AppConfig, GlobalBootstrapGrants,
    GlobalBootstrapTunnelAuthorizer, GlobalIdentityCache, GlobalIdentityJoinApprover,
    GlobalIdentityRevisionWriter, LocalSetup, LocalSetupJoin, RouterState, SetupJoinExecutor,
    SetupNewExecutor, SetupStage, TimedPairingAcceptanceController,
    router::{HostedStorageScopeResolver, openapi_router},
    storage_router,
};

type LocalOAuth2Service = OAuth2Service<
    LibSqlApplicationRepo,
    LibSqlClientRepo,
    LibSqlOAuth2AuthorizationCodeRepo,
    LibSqlUserRepo,
    LibSqlOAuth2UserConsentRepo,
    LibSqlKeyRepo,
>;
type CliLocalTunnelAuthorizer = LocalTunnelAuthorizer<LocalOAuth2Service, LibSqlDeviceRepo>;
type CliLocalTunnelAuthorizationProvider =
    LocalTunnelAuthorizationProvider<LocalOAuth2Service, LibSqlDeviceRepo>;

enum CliTunnelAuthorizer {
    Hosted(
        HostedTunnelAuthorizer,
        Arc<GlobalBootstrapGrants>,
        Arc<crate::GlobalIdentityReadGateSlot>,
    ),
    Local(
        CliLocalTunnelAuthorizer,
        Arc<GlobalBootstrapGrants>,
        Arc<crate::GlobalIdentityReadGateSlot>,
    ),
}

type CliGlobalTransport =
    ScopedIrohTransport<DynamicEndpointIdStore, CliTunnelAuthorizer, StaticTunnelAuthorization>;
type CliGlobalRuntime = GlobalIdentityRuntime<EndpointIdCodec, CliGlobalTransport>;

struct CliSetupJoinExecutor {
    manager: Server<DynamicEndpointIdStore, CliTunnelAuthorizer>,
    allowlist: DynamicEndpointIdStore,
    cache: GlobalIdentityCache,
    root: PathBuf,
}

struct CliSetupNewExecutor {
    runtime: Arc<CliGlobalRuntime>,
    cache: GlobalIdentityCache,
    data_dir: PathBuf,
    bootstrap_config: BootstrapConfig,
    password_config: idp_service::PasswordConfig,
    issuer: String,
    key_namespace: String,
}

type IsolatedBootstrapService = BootstrapService<
    LibSqlApplicationRepo,
    LibSqlClientRepo,
    LibSqlKeyRepo,
    LibSqlUserRepo,
    LibSqlRoleRepo,
    LibSqlPermissionRepo,
    LibSqlDeviceRepo,
>;

impl SetupNewExecutor for CliSetupNewExecutor {
    fn bootstrap(
        &self,
        input: BootstrapInput,
        device: (String, String),
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move {
            if self.runtime.active_rows().await?.is_some() {
                return Err("global identity baseline already exists".to_owned());
            }
            let database_path = self.data_dir.join(format!(
                "global-identity-bootstrap-{}.db",
                generate_random_string::<32>()
            ));
            let database = Arc::new(
                libsql::Builder::new_local(&database_path)
                    .build()
                    .await
                    .map_err(|error| error.to_string())?,
            );
            idp_model::migrate::up(&database)
                .await
                .map_err(|error| error.to_string())?;
            let key_service = Arc::new(KeyService::new(
                LibSqlKeyRepo::new(Arc::clone(&database)),
                PrivateKeyKeyringRepo::new(&self.issuer),
                self.key_namespace.clone(),
            ));
            let bootstrap: IsolatedBootstrapService = BootstrapService::new(
                LibSqlApplicationRepo::new(Arc::clone(&database)),
                LibSqlClientRepo::new(Arc::clone(&database), Arc::clone(&key_service)),
                LibSqlUserRepo::new(
                    Arc::clone(&database),
                    Arc::clone(&key_service),
                    self.password_config.clone(),
                ),
                LibSqlRoleRepo::new(Arc::clone(&database)),
                LibSqlPermissionRepo::new(Arc::clone(&database)),
                LibSqlDeviceRepo::new(Arc::clone(&database)),
                key_service,
                self.bootstrap_config.clone(),
            );
            bootstrap
                .ensure_system_baseline(&input, Some(device))
                .await
                .map_err(|error| error.to_string())?;
            let rows = GlobalIdentityCache::new(Arc::clone(&database))
                .snapshot()
                .await?;
            let revision = format!("bootstrap-{}", generate_random_string::<32>());
            GlobalIdentityRevisionWriter::new(Arc::clone(&self.runtime), self.cache.clone())
                .apply(revision, |snapshot| {
                    *snapshot = rows;
                    Ok(())
                })
                .await?;
            db::close_database(&database)
                .await
                .map_err(|error| error.to_string())?;
            std::fs::remove_file(database_path).map_err(|error| error.to_string())?;
            Ok(())
        })
    }
}

impl SetupJoinExecutor for CliSetupJoinExecutor {
    fn join(
        &self,
        setup: Arc<LocalSetup>,
        join: LocalSetupJoin,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move {
            let offer = idp_model::contract::GlobalIdentityJoinOffer {
                device_name: join.device_name,
                joining_endpoint_addr: serde_json::to_string(&self.manager.endpoint().addr())
                    .map_err(|error| error.to_string())?,
                joining_public_key: join.joining_public_key.clone(),
                nonce: join.nonce.clone(),
            };
            let endpoint: EndpointAddr =
                serde_json::from_str(&join.endpoint_addr).map_err(|error| error.to_string())?;
            let reply: idp_model::contract::GlobalIdentityJoinReply = serde_json::from_slice(
                &self
                    .manager
                    .send_pairing_offer(
                        endpoint,
                        &serde_json::to_vec(&offer).map_err(|error| error.to_string())?,
                    )
                    .await
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_secs();
            let now = i64::try_from(now).unwrap_or(i64::MAX);
            if !reply.is_valid_for(&join.joining_public_key, &join.nonce, now) {
                return Err("invalid global identity join reply".to_owned());
            }
            let accepting: EndpointAddr = serde_json::from_str(&reply.accepting_endpoint_addr)
                .map_err(|error| error.to_string())?;
            if accepting.id.to_string() != reply.grant.accepting_public_key {
                return Err("global identity join reply endpoint mismatch".to_owned());
            }
            self.allowlist
                .insert_scope("global-identity".to_owned(), accepting.id)
                .await;
            let authorization =
                serde_json::to_vec(&reply.grant).map_err(|error| error.to_string())?;
            let transport = ScopedIrohTransport::new(
                self.manager.clone(),
                VaultId::global_identity(),
                StaticTunnelAuthorization::new(authorization),
            );
            let runtime = Arc::new(
                CliGlobalRuntime::new(
                    self.root.clone(),
                    self.manager.endpoint().id(),
                    transport.clone(),
                )
                .await?,
            );
            let peer = transport
                .connect(accepting)
                .await
                .map_err(|error| error.to_string())?;
            runtime.synchronize(peer).await?;
            let manifest = runtime.activate_revision(&reply.target_revision).await?;
            let Some((active, rows)) = runtime.active_rows().await? else {
                return Err("missing active global identity revision".to_owned());
            };
            if active != manifest {
                return Err("global identity activation mismatch".to_owned());
            }
            self.cache.apply(&manifest, &rows).await?;
            if !runtime
                .has_approved_device(&join.joining_public_key)
                .await?
            {
                return Err("local device is not approved in global identity".to_owned());
            }
            setup
                .advance(SetupStage::Device)
                .map_err(|error| error.to_string())
        })
    }
}

impl TunnelAuthorizer for CliTunnelAuthorizer {
    async fn authorize(
        &self,
        vault_id: VaultId,
        initiating_id: EndpointId,
        accepting_id: EndpointId,
        authorization: &[u8],
    ) -> bool {
        if vault_id == VaultId::global_identity() {
            let grants = match self {
                Self::Hosted(_, grants, _) | Self::Local(_, grants, _) => Arc::clone(grants),
            };
            return GlobalBootstrapTunnelAuthorizer::new(grants)
                .authorize(vault_id, initiating_id, accepting_id, authorization)
                .await;
        }
        let gate = match self {
            Self::Hosted(_, _, gate) | Self::Local(_, _, gate) => gate,
        };
        if !gate.verify().await {
            return false;
        }
        match self {
            Self::Hosted(authorizer, _, _) => {
                authorizer
                    .authorize(vault_id, initiating_id, accepting_id, authorization)
                    .await
            }
            Self::Local(authorizer, _, _) => {
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
    Local(Arc<CliLocalTunnelAuthorizer>),
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
    Local(CliLocalTunnelAuthorizationProvider),
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

    idp_model::migrate::up(&database).await.map_err(|e| {
        log::error!("failed to run database migrations: {}", e);
        io::Error::other(e)
    })?;

    let database = Arc::new(database);
    let key_service = Arc::new(KeyService::new(
        LibSqlKeyRepo::new(database.clone()),
        PrivateKeyKeyringRepo::new(&app_config.oauth2.issuer),
        app_config.key_namespace.clone(),
    ));

    let devices = Arc::new(LibSqlDeviceRepo::new(database.clone()));

    let setup_state = crate::LocalSetupState::load_or_create(&app_config.data_dir)?;
    let device_identity = Arc::new(crate::open_device_identity(&setup_state).await?);

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
        database.clone(),
        Arc::clone(&oauth2_service),
        storage_sessions.clone(),
        Arc::clone(&devices),
        Arc::clone(&device_identity),
    )
    .with_local_setup(&app_config.data_dir, setup_state);
    let router_state = match &control_plane {
        Some(control_plane) => router_state
            .with_hosted_control_plane(Arc::clone(control_plane))
            .with_storage_scope_resolver(Arc::new(HostedStorageScopeResolver::new(Arc::clone(
                control_plane,
            )))),
        None => router_state,
    };
    if let Some(token) = router_state.setup_token() {
        log::info!("Setup token: {token}");
    }
    let storage_root = PathBuf::from(&args.config)
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let allowlist = DynamicEndpointIdStore::default();
    let global_grants = Arc::new(GlobalBootstrapGrants::new());
    let authorizer = match &control_plane {
        Some(control_plane) => CliTunnelAuthorizer::Hosted(
            HostedTunnelAuthorizer::new(Arc::clone(control_plane)),
            Arc::clone(&global_grants),
            Arc::clone(&router_state.global_identity_read_gate),
        ),
        None => CliTunnelAuthorizer::Local(
            LocalTunnelAuthorizer::new(Arc::clone(&oauth2_service), Arc::clone(&devices)),
            Arc::clone(&global_grants),
            Arc::clone(&router_state.global_identity_read_gate),
        ),
    };
    let manager = Server::new(device_identity.endpoint(), allowlist.clone(), authorizer);
    router_state
        .pairing_acceptance
        .bind(Arc::new(TimedPairingAcceptanceController::new(
            manager.clone(),
            Duration::from_secs(app_config.pairing.accepting_timeout_seconds),
        )))
        .map_err(io::Error::other)?;
    let global_transport = ScopedIrohTransport::new(
        manager.clone(),
        VaultId::global_identity(),
        StaticTunnelAuthorization::new(Vec::new()),
    );
    let global_runtime = Arc::new(
        CliGlobalRuntime::new(
            app_config.data_dir.clone().into(),
            device_identity.endpoint_id(),
            global_transport,
        )
        .await
        .map_err(io::Error::other)?,
    );
    let global_cache = GlobalIdentityCache::new(Arc::clone(&database));
    let join_approver = Arc::new(GlobalIdentityJoinApprover::new(
        Arc::clone(&global_runtime),
        global_cache.clone(),
        Arc::clone(&global_grants),
        device_identity.endpoint_id().to_string(),
        device_identity
            .endpoint_address()
            .map_err(io::Error::other)?,
    ));
    let mut pairing_offers = manager.subscribe_pairing_offers();
    let pairing_allowlist = allowlist.clone();
    let pairing_runtime = Arc::clone(&global_runtime);
    let pairing_cache = global_cache.clone();
    spawn(async move {
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
    let router_state = router_state
        .with_global_identity_read_gate(Arc::new(ActiveGlobalIdentityReadGate::new(
            Arc::clone(&global_runtime),
            global_cache.clone(),
        )))
        .with_setup_new_executor(Arc::new(CliSetupNewExecutor {
            runtime: Arc::clone(&global_runtime),
            cache: global_cache.clone(),
            data_dir: app_config.data_dir.clone().into(),
            bootstrap_config: app_config.bootstrap.clone(),
            password_config: app_config.password.clone(),
            issuer: app_config.oauth2.issuer.clone(),
            key_namespace: app_config.key_namespace.clone(),
        }))
        .with_setup_join_executor(Arc::new(CliSetupJoinExecutor {
            manager: manager.clone(),
            allowlist: allowlist.clone(),
            cache: global_cache,
            root: app_config.data_dir.clone().into(),
        }));
    let listener = manager.clone();
    spawn(async move {
        listener.listen().await;
    });
    log::info!("Iroh endpoint: {:?}", device_identity.endpoint().addr());
    let authorization_provider = match &control_plane {
        Some(control_plane) => CliAuthorizationProvider::Hosted(Arc::clone(control_plane)),
        None => CliAuthorizationProvider::Local(Arc::new(LocalTunnelAuthorizer::new(
            Arc::clone(&oauth2_service),
            Arc::clone(&devices),
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
