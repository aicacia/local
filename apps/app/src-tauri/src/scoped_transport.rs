use std::{
    future::Future,
    io::{Error, ErrorKind},
    pin::Pin,
    sync::Arc,
};

use idp_model::contract::TunnelAuthorizationRequest;
use iroh::{EndpointAddr, EndpointId};
use iroh_chain::{DynamicEndpointIdStore, VaultId};
use management_service::StorageScope;
use storage_service::{
    DeferredIrohTransportFactory, IrohTransport, IrohTransportFactory, ScopedFileSystemRuntime,
    ScopedTunnelAuthorizationProvider, TrustedEndpointAddrLookup,
};

use crate::tunnel_authorizer::{
    DeviceTunnelManager, LidpTunnelAuthorizer, LocalTunnel, LocalTunnelProvider,
};
use management_service::HostedControlPlane;

type AppTransport = IrohTransport<LidpTunnelAuthorizer, AppTunnelAuthorization>;
type AppTransportFactory = DeferredIrohTransportFactory<
    LidpTunnelAuthorizer,
    AppTunnelAuthorizationProvider,
    AppTrustedEndpointLookup,
    StorageScope,
>;
pub type AppFileSystemRuntime = ScopedFileSystemRuntime<
    iroh_chain_file_system::EndpointIdCodec,
    AppTransport,
    AppTransportFactory,
    StorageScope,
>;

#[derive(Clone, Default)]
pub struct TunnelContext {
    factory: AppTransportFactory,
}

impl TunnelContext {
    pub(crate) fn transport_factory(&self) -> AppTransportFactory {
        self.factory.clone()
    }

    pub fn set(
        &self,
        manager: DeviceTunnelManager,
        allowlist: DynamicEndpointIdStore,
        local: Arc<LocalTunnel>,
        control_plane: Option<Arc<HostedControlPlane>>,
    ) {
        self.factory.set(IrohTransportFactory::new(
            manager,
            allowlist,
            AppTunnelAuthorizationProvider {
                local,
                control_plane: control_plane.clone(),
            },
            AppTrustedEndpointLookup { control_plane },
        ));
    }
}

#[derive(Clone)]
pub(crate) struct AppTunnelAuthorizationProvider {
    local: Arc<LocalTunnel>,
    control_plane: Option<Arc<HostedControlPlane>>,
}

impl ScopedTunnelAuthorizationProvider<StorageScope> for AppTunnelAuthorizationProvider {
    type Authorization = AppTunnelAuthorization;

    fn authorization(&self, scope: &StorageScope) -> Result<Self::Authorization, String> {
        match &self.control_plane {
            Some(control_plane) => Ok(AppTunnelAuthorization::Hosted(HostedTunnelAuthorization {
                scope: scope.clone(),
                control_plane: Arc::clone(control_plane),
            })),
            None => Ok(AppTunnelAuthorization::Local(
                self.local.authorization_provider(scope.clone()),
            )),
        }
    }
}

#[derive(Clone)]
pub(crate) struct AppTrustedEndpointLookup {
    control_plane: Option<Arc<HostedControlPlane>>,
}

impl TrustedEndpointAddrLookup<StorageScope> for AppTrustedEndpointLookup {
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

#[derive(Clone)]
pub(crate) enum AppTunnelAuthorization {
    Local(LocalTunnelProvider),
    Hosted(HostedTunnelAuthorization),
}

impl iroh_chain_file_system::TunnelAuthorizationProvider for AppTunnelAuthorization {
    fn authorization(
        &self,
        vault_id: VaultId,
        local_id: EndpointId,
        remote_id: EndpointId,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, Error>> + Send + '_>> {
        match self {
            Self::Local(authorization) => {
                authorization.authorization(vault_id, local_id, remote_id)
            }
            Self::Hosted(authorization) => {
                authorization.authorization(vault_id, local_id, remote_id)
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct HostedTunnelAuthorization {
    scope: StorageScope,
    control_plane: Arc<HostedControlPlane>,
}

impl iroh_chain_file_system::TunnelAuthorizationProvider for HostedTunnelAuthorization {
    fn authorization(
        &self,
        vault_id: VaultId,
        local_id: EndpointId,
        remote_id: EndpointId,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, Error>> + Send + '_>> {
        let scope = self.scope.clone();
        let control_plane = Arc::clone(&self.control_plane);
        Box::pin(async move {
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
            Ok(authorization.token.into_bytes())
        })
    }
}
