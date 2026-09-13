use std::{future::Future, path::PathBuf, pin::Pin, sync::Arc};

use bootstrap_service::bootstrap::{BootstrapConfig, BootstrapInput, BootstrapService};
use db::close_database;
use idp_server::{
    GlobalIdentityCache, GlobalIdentityRevisionWriter, LocalSetup, LocalSetupJoin,
    SetupJoinExecutor, SetupNewExecutor, SetupStage,
};
use idp_service::{
    PasswordConfig, generate_random_string,
    libsql::{LibSqlApplicationRepo, LibSqlClientRepo, LibSqlKeyRepo, LibSqlUserRepo},
    repo::{KeyService, PrivateKeyKeyringRepo},
};
use iroh::EndpointAddr;
use iroh_chain::{DynamicEndpointIdStore, VaultId};
use iroh_chain_file_system::{EndpointIdCodec, ScopedIrohTransport, StaticTunnelAuthorization};
use management_service::libsql::{LibSqlDeviceRepo, LibSqlPermissionRepo, LibSqlRoleRepo};
use storage_service::GlobalIdentityRuntime;

use crate::tunnel_authorizer::{DeviceTunnelManager, LidpTunnelAuthorizer};

type DesktopGlobalTransport =
    ScopedIrohTransport<DynamicEndpointIdStore, LidpTunnelAuthorizer, StaticTunnelAuthorization>;
pub type DesktopGlobalRuntime = GlobalIdentityRuntime<EndpointIdCodec, DesktopGlobalTransport>;
type IsolatedBootstrapService = BootstrapService<
    LibSqlApplicationRepo,
    LibSqlClientRepo,
    LibSqlKeyRepo,
    LibSqlUserRepo,
    LibSqlRoleRepo,
    LibSqlPermissionRepo,
    LibSqlDeviceRepo,
>;

pub struct DesktopSetupNewExecutor {
    runtime: Arc<DesktopGlobalRuntime>,
    cache: GlobalIdentityCache,
    data_dir: PathBuf,
    bootstrap_config: BootstrapConfig,
    password_config: PasswordConfig,
    issuer: String,
    key_namespace: String,
}

impl DesktopSetupNewExecutor {
    #[must_use]
    pub fn new(
        runtime: Arc<DesktopGlobalRuntime>,
        cache: GlobalIdentityCache,
        data_dir: PathBuf,
        bootstrap_config: BootstrapConfig,
        password_config: PasswordConfig,
        issuer: String,
        key_namespace: String,
    ) -> Self {
        Self {
            runtime,
            cache,
            data_dir,
            bootstrap_config,
            password_config,
            issuer,
            key_namespace,
        }
    }
}

impl SetupNewExecutor for DesktopSetupNewExecutor {
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
            GlobalIdentityRevisionWriter::new(Arc::clone(&self.runtime), self.cache.clone())
                .apply(
                    format!("bootstrap-{}", generate_random_string::<32>()),
                    |snapshot| {
                        *snapshot = rows;
                        Ok(())
                    },
                )
                .await?;
            close_database(&database)
                .await
                .map_err(|error| error.to_string())?;
            std::fs::remove_file(database_path).map_err(|error| error.to_string())?;
            Ok(())
        })
    }
}

pub struct DesktopSetupJoinExecutor {
    manager: DeviceTunnelManager,
    allowlist: DynamicEndpointIdStore,
    cache: GlobalIdentityCache,
    root: PathBuf,
}

impl DesktopSetupJoinExecutor {
    #[must_use]
    pub fn new(
        manager: DeviceTunnelManager,
        allowlist: DynamicEndpointIdStore,
        cache: GlobalIdentityCache,
        root: PathBuf,
    ) -> Self {
        Self {
            manager,
            allowlist,
            cache,
            root,
        }
    }
}

impl SetupJoinExecutor for DesktopSetupJoinExecutor {
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
            let transport = ScopedIrohTransport::new(
                self.manager.clone(),
                VaultId::global_identity(),
                StaticTunnelAuthorization::new(
                    serde_json::to_vec(&reply.grant).map_err(|error| error.to_string())?,
                ),
            );
            let runtime = Arc::new(
                DesktopGlobalRuntime::new(
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
