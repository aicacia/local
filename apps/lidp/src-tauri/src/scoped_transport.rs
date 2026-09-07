use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    io::{Error, ErrorKind},
    pin::Pin,
    sync::{Arc, Mutex},
};

use iroh::{EndpointAddr, EndpointId};
use iroh_chain::{DynamicEndpointIdStore, VaultId};
use iroh_chain_file_system::{EndpointIdCodec, ScopedIrohTransport, TunnelAuthorizationProvider};
use lidp_model::contract::TunnelAuthorizationRequest;
use lidp_service::{
    repo::UserDeviceRepo,
    scoped_file_system::{ScopedFileSystem, ScopedFileSystemRuntime, ScopedTransportFactory},
    storage_session::StorageScope,
};

use crate::hosted_control_plane::HostedControlPlane;
use crate::tunnel_authorizer::{DeviceTunnelManager, LidpTunnelAuthorizer};

type AppTransport =
    ScopedIrohTransport<DynamicEndpointIdStore, LidpTunnelAuthorizer, LidpTunnelAuthorization>;
pub type AppFileSystemRuntime =
    ScopedFileSystemRuntime<EndpointIdCodec, AppTransport, IrohTransportFactory>;
type TunnelParts = (
    Arc<DeviceTunnelManager>,
    Arc<DynamicEndpointIdStore>,
    Arc<lidp_server::RouterState>,
    Option<Arc<HostedControlPlane>>,
);

#[derive(Clone, Default)]
pub struct TunnelContext {
    inner: Arc<Mutex<Option<TunnelParts>>>,
}

impl TunnelContext {
    pub fn set(
        &self,
        manager: Arc<DeviceTunnelManager>,
        allowlist: Arc<DynamicEndpointIdStore>,
        state: Arc<lidp_server::RouterState>,
        control_plane: Option<Arc<HostedControlPlane>>,
    ) {
        *self.inner.lock().expect("tunnel context lock poisoned") =
            Some((manager, allowlist, state, control_plane));
    }

    fn get(&self) -> Result<TunnelParts, String> {
        self.inner
            .lock()
            .expect("tunnel context lock poisoned")
            .clone()
            .ok_or_else(|| "tunnel manager is not initialized".to_owned())
    }
}

#[derive(Clone)]
pub struct IrohTransportFactory {
    context: TunnelContext,
    transports: Arc<Mutex<BTreeMap<String, AppTransport>>>,
    listeners: Arc<Mutex<BTreeSet<String>>>,
}

impl IrohTransportFactory {
    pub fn new(context: TunnelContext) -> Self {
        Self {
            context,
            transports: Arc::new(Mutex::new(BTreeMap::new())),
            listeners: Arc::new(Mutex::new(BTreeSet::new())),
        }
    }
}

impl ScopedTransportFactory<EndpointIdCodec, AppTransport> for IrohTransportFactory {
    fn create(&self, scope: &StorageScope) -> Result<AppTransport, String> {
        let (manager, _, state, control_plane) = self.context.get()?;
        let vault_id = VaultId::from_application(&scope.user_sub, scope.application_id);
        let transport = AppTransport::new(
            (*manager).clone(),
            vault_id,
            LidpTunnelAuthorization {
                state,
                scope: scope.clone(),
                control_plane,
            },
        );
        self.transports
            .lock()
            .expect("transport map lock poisoned")
            .insert(scope_key(scope), transport.clone());
        Ok(transport)
    }

    fn synchronize(
        &self,
        scope: StorageScope,
        file_system: Arc<ScopedFileSystem<EndpointIdCodec, AppTransport>>,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
        let context = self.context.clone();
        let scope_key = scope_key(&scope);
        let transport = self
            .transports
            .lock()
            .expect("transport map lock poisoned")
            .get(&scope_key)
            .cloned();
        let watch_peers = self
            .listeners
            .lock()
            .expect("transport listeners lock poisoned")
            .insert(scope_key.clone());
        Box::pin(async move {
            let (manager, allowlist, _, control_plane) = context.get()?;
            let transport = transport.ok_or_else(|| "transport is missing".to_owned())?;
            let local_id = manager.endpoint().id();
            let trusted_devices = match control_plane {
                Some(control_plane) => control_plane.trusted_devices(&scope.access_token).await?,
                None => scope.trusted_devices,
            };
            let endpoints = trusted_devices
                .iter()
                .filter_map(|device| {
                    let endpoint = serde_json::from_str::<EndpointAddr>(&device.address).ok()?;
                    (endpoint.id != local_id).then_some(endpoint)
                })
                .collect::<Vec<_>>();
            allowlist
                .replace_scope(
                    scope_key.clone(),
                    endpoints.iter().map(|endpoint| endpoint.id),
                )
                .await;
            manager.close_disallowed().await;
            for peer in transport.peers() {
                let _ = file_system.sync_peer(peer).await;
            }
            if watch_peers {
                let mut peer_events = transport.subscribe_peers();
                let synced_file_system = Arc::clone(&file_system);
                tokio::spawn(async move {
                    while let Ok(peer) = peer_events.recv().await {
                        let _ = synced_file_system.sync_peer(peer).await;
                    }
                });
            }
            for endpoint in endpoints {
                if let Ok(peer) = transport.connect(endpoint).await {
                    let _ = file_system.sync_peer(peer).await;
                }
            }
            Ok(())
        })
    }
}

#[derive(Clone)]
pub(crate) struct LidpTunnelAuthorization {
    state: Arc<lidp_server::RouterState>,
    scope: StorageScope,
    control_plane: Option<Arc<HostedControlPlane>>,
}

impl TunnelAuthorizationProvider for LidpTunnelAuthorization {
    fn authorization(
        &self,
        vault_id: VaultId,
        local_id: EndpointId,
        remote_id: EndpointId,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, Error>> + Send + '_>> {
        let state = Arc::clone(&self.state);
        let scope = self.scope.clone();
        let control_plane = self.control_plane.clone();
        Box::pin(async move {
            if let Some(control_plane) = control_plane {
                let authorization = control_plane
                    .tunnel_authorization(
                        &scope.access_token,
                        TunnelAuthorizationRequest {
                            vault_id_hash: vault_id.hash(),
                            local_public_key: local_id.to_string(),
                            remote_public_key: remote_id.to_string(),
                        },
                    )
                    .await
                    .map_err(|_| Error::new(ErrorKind::PermissionDenied, "grant was rejected"))?;
                return Ok(authorization.token.into_bytes());
            }
            let approved = state
                .user_devices
                .are_approved_by_user_id(
                    scope.user_sub.parse().map_err(|_| {
                        Error::new(ErrorKind::PermissionDenied, "invalid user subject")
                    })?,
                    &local_id.to_string(),
                    &remote_id.to_string(),
                )
                .await
                .map_err(|error| Error::other(error.to_string()))?;
            if !approved {
                return Err(Error::new(
                    ErrorKind::PermissionDenied,
                    "peer is not approved",
                ));
            }
            let principal = state
                .oauth2_service
                .find_principal(scope.principal_key_id)
                .await
                .map_err(|_| Error::other("principal lookup failed"))?
                .ok_or_else(|| Error::new(ErrorKind::PermissionDenied, "principal is missing"))?;
            let authorization = state
                .oauth2_service
                .issue_tunnel_authorization(
                    principal.as_ref(),
                    scope.application_id,
                    TunnelAuthorizationRequest {
                        vault_id_hash: vault_id.hash(),
                        local_public_key: local_id.to_string(),
                        remote_public_key: remote_id.to_string(),
                    },
                )
                .await
                .map_err(|_| Error::new(ErrorKind::PermissionDenied, "grant was rejected"))?;
            Ok(authorization.token.into_bytes())
        })
    }
}

fn scope_key(scope: &StorageScope) -> String {
    format!("{}:{}", scope.user_sub, scope.application_id)
}
