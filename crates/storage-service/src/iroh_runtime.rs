use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use crate::{ScopedFileSystem, ScopedTransportFactory, StorageNamespace};
use iroh::EndpointAddr;
use iroh_chain::{DynamicEndpointIdStore, Server, TunnelAuthorizer, VaultId};
use iroh_chain_file_system::{AccessTokenProvider, EndpointIdCodec, ScopedIrohTransport};

pub type IrohTransport<A, P> = ScopedIrohTransport<DynamicEndpointIdStore, A, P>;
type IrohFactory<A, P, L, S> = IrohTransportFactory<A, P, L, S>;
type DeferredIrohFactory<A, P, L, S> = Arc<Mutex<Option<Arc<IrohFactory<A, P, L, S>>>>>;
type IrohTransports<A, P> = Arc<Mutex<BTreeMap<String, IrohTransport<A, P>>>>;

pub trait ScopedAccessTokenProvider<S>: Send + Sync + 'static {
    type Authorization: AccessTokenProvider;

    fn authorization(&self, scope: &S) -> Result<Self::Authorization, String>;
}

pub trait TrustedEndpointAddrLookup<S>: Send + Sync + 'static {
    fn trusted_endpoint_addrs(
        &self,
        scope: &S,
    ) -> impl Future<Output = Result<Vec<EndpointAddr>, String>> + Send;
}

pub struct DeferredIrohTransportFactory<A, P, L, S>
where
    A: TunnelAuthorizer,
    P: ScopedAccessTokenProvider<S>,
    L: TrustedEndpointAddrLookup<S>,
{
    inner: DeferredIrohFactory<A, P, L, S>,
}

impl<A, P, L, S> Clone for DeferredIrohTransportFactory<A, P, L, S>
where
    A: TunnelAuthorizer,
    P: ScopedAccessTokenProvider<S>,
    L: TrustedEndpointAddrLookup<S>,
{
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<A, P, L, S> Default for DeferredIrohTransportFactory<A, P, L, S>
where
    A: TunnelAuthorizer,
    P: ScopedAccessTokenProvider<S>,
    L: TrustedEndpointAddrLookup<S>,
{
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
        }
    }
}

impl<A, P, L, S> DeferredIrohTransportFactory<A, P, L, S>
where
    A: TunnelAuthorizer,
    P: ScopedAccessTokenProvider<S>,
    L: TrustedEndpointAddrLookup<S>,
{
    pub fn set(&self, factory: IrohTransportFactory<A, P, L, S>) {
        *self
            .inner
            .lock()
            .expect("Iroh transport factory lock poisoned") = Some(Arc::new(factory));
    }

    fn get(&self) -> Result<Arc<IrohTransportFactory<A, P, L, S>>, String> {
        self.inner
            .lock()
            .expect("Iroh transport factory lock poisoned")
            .clone()
            .ok_or_else(|| "Iroh transport factory is not initialized".to_owned())
    }
}

impl<A, P, L, S> ScopedTransportFactory<EndpointIdCodec, IrohTransport<A, P::Authorization>, S>
    for DeferredIrohTransportFactory<A, P, L, S>
where
    A: TunnelAuthorizer,
    P: ScopedAccessTokenProvider<S>,
    L: TrustedEndpointAddrLookup<S>,
    S: StorageNamespace + Send + 'static,
{
    fn create(&self, scope: &S) -> Result<IrohTransport<A, P::Authorization>, String> {
        self.get()?.create(scope)
    }

    fn synchronize(
        &self,
        scope: S,
        file_system: Arc<ScopedFileSystem<EndpointIdCodec, IrohTransport<A, P::Authorization>>>,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
        let factory = self.get();
        Box::pin(async move { factory?.synchronize(scope, file_system).await })
    }
}

pub struct IrohTransportFactory<A, P, L, S>
where
    A: TunnelAuthorizer,
    P: ScopedAccessTokenProvider<S>,
    L: TrustedEndpointAddrLookup<S>,
{
    manager: Server<DynamicEndpointIdStore, A>,
    allowlist: DynamicEndpointIdStore,
    authorization_provider: P,
    trusted_endpoint_addrs: L,
    transports: IrohTransports<A, P::Authorization>,
    listeners: Arc<Mutex<BTreeSet<String>>>,
    _scope: core::marker::PhantomData<fn(S)>,
}

impl<A, P, L, S> IrohTransportFactory<A, P, L, S>
where
    A: TunnelAuthorizer,
    P: ScopedAccessTokenProvider<S>,
    L: TrustedEndpointAddrLookup<S>,
{
    pub fn new(
        manager: Server<DynamicEndpointIdStore, A>,
        allowlist: DynamicEndpointIdStore,
        authorization_provider: P,
        trusted_endpoint_addrs: L,
    ) -> Self {
        Self {
            manager,
            allowlist,
            authorization_provider,
            trusted_endpoint_addrs,
            transports: Arc::new(Mutex::new(BTreeMap::new())),
            listeners: Arc::new(Mutex::new(BTreeSet::new())),
            _scope: core::marker::PhantomData,
        }
    }
}

impl<A, P, L, S> ScopedTransportFactory<EndpointIdCodec, IrohTransport<A, P::Authorization>, S>
    for IrohTransportFactory<A, P, L, S>
where
    A: TunnelAuthorizer,
    P: ScopedAccessTokenProvider<S>,
    L: TrustedEndpointAddrLookup<S>,
    S: StorageNamespace + Send + 'static,
{
    fn create(&self, scope: &S) -> Result<IrohTransport<A, P::Authorization>, String> {
        let transport = IrohTransport::new(
            self.manager.clone(),
            VaultId::from_application(scope.user_sub(), scope.application_id()),
            self.authorization_provider.authorization(scope)?,
        );
        self.transports
            .lock()
            .expect("transport map lock poisoned")
            .insert(scope_key(scope), transport.clone());
        Ok(transport)
    }

    fn synchronize(
        &self,
        scope: S,
        file_system: Arc<ScopedFileSystem<EndpointIdCodec, IrohTransport<A, P::Authorization>>>,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
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
            let transport = transport.ok_or_else(|| "transport is missing".to_owned())?;
            let local_id = self.manager.endpoint().id();
            let endpoints = self
                .trusted_endpoint_addrs
                .trusted_endpoint_addrs(&scope)
                .await?
                .into_iter()
                .filter(|endpoint| endpoint.id != local_id)
                .collect::<Vec<_>>();
            self.allowlist
                .replace_scope(scope_key, endpoints.iter().map(|endpoint| endpoint.id))
                .await;
            self.manager.close_disallowed().await;
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

fn scope_key(scope: &impl StorageNamespace) -> String {
    format!("{}:{}", scope.user_sub(), scope.application_id())
}
