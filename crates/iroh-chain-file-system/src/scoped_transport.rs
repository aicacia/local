use std::{
    collections::BTreeMap,
    future::Future,
    io::{Error, ErrorKind},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use file_system::Transport;
use iroh::{EndpointAddr, EndpointId};
use iroh_chain::{AllowedEndpointId, Server, Tunnel, TunnelAuthorizer, VaultId};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Mutex as AsyncMutex, broadcast, mpsc},
};

const INCOMING_CAPACITY: usize = 64;
const MAX_FRAME_SIZE: usize = 1024 * 1024;

type Message = (EndpointId, Vec<u8>);

pub trait TunnelAuthorizationProvider: Send + Sync + 'static {
    fn authorization(
        &self,
        vault_id: VaultId,
        local_id: EndpointId,
        remote_id: EndpointId,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, Error>> + Send + '_>>;
}

#[derive(Clone)]
pub struct StaticTunnelAuthorization(Vec<u8>);

impl StaticTunnelAuthorization {
    pub fn new(authorization: Vec<u8>) -> Self {
        Self(authorization)
    }
}

impl TunnelAuthorizationProvider for StaticTunnelAuthorization {
    fn authorization(
        &self,
        _: VaultId,
        _: EndpointId,
        _: EndpointId,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, Error>> + Send + '_>> {
        Box::pin(async { Ok(self.0.clone()) })
    }
}

pub struct ScopedIrohIncoming(mpsc::Receiver<Message>);

impl futures_core::Stream for ScopedIrohIncoming {
    type Item = Message;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.poll_recv(context)
    }
}

pub struct ScopedIrohTransport<A, V, P>
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
    P: TunnelAuthorizationProvider,
{
    inner: Arc<ScopedIrohTransportInner<A, V, P>>,
}

struct ScopedIrohTransportInner<A, V, P>
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
    P: TunnelAuthorizationProvider,
{
    manager: Server<A, V>,
    vault_id: VaultId,
    authorization: P,
    peers: Mutex<BTreeMap<EndpointId, Tunnel>>,
    incoming: AsyncMutex<Option<ScopedIrohIncoming>>,
    incoming_tx: mpsc::Sender<Message>,
    peer_events: broadcast::Sender<EndpointId>,
}

impl<A, V, P> ScopedIrohTransport<A, V, P>
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
    P: TunnelAuthorizationProvider,
{
    pub fn new(manager: Server<A, V>, vault_id: VaultId, authorization: P) -> Self {
        let (incoming_tx, incoming_rx) = mpsc::channel(INCOMING_CAPACITY);
        let (peer_events, _) = broadcast::channel(INCOMING_CAPACITY);
        let inner = Arc::new(ScopedIrohTransportInner {
            manager: manager.clone(),
            vault_id,
            authorization,
            peers: Mutex::new(BTreeMap::new()),
            incoming: AsyncMutex::new(Some(ScopedIrohIncoming(incoming_rx))),
            incoming_tx,
            peer_events,
        });
        let task_inner = Arc::clone(&inner);
        let mut events = manager.subscribe();
        tokio::spawn(async move {
            loop {
                match events.recv().await {
                    Ok(event) if event.vault_id == task_inner.vault_id => {
                        add_tunnel(&task_inner, event.tunnel).await;
                    }
                    Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });
        Self { inner }
    }

    pub async fn connect(&self, endpoint: impl Into<EndpointAddr>) -> Result<EndpointId, Error> {
        let endpoint = endpoint.into();
        let authorization = self
            .inner
            .authorization
            .authorization(
                self.inner.vault_id,
                self.inner.manager.endpoint().id(),
                endpoint.id,
            )
            .await?;
        let tunnel = self
            .inner
            .manager
            .connect(self.inner.vault_id, endpoint, &authorization)
            .await?;
        let peer_id = tunnel.remote_id();
        add_tunnel(&self.inner, tunnel).await;
        Ok(peer_id)
    }

    pub fn subscribe_peers(&self) -> broadcast::Receiver<EndpointId> {
        self.inner.peer_events.subscribe()
    }

    pub fn peers(&self) -> Vec<EndpointId> {
        self.inner
            .peers
            .lock()
            .expect("peer lock poisoned")
            .keys()
            .copied()
            .collect()
    }

    pub async fn disconnect(&self, peer_id: EndpointId) -> bool {
        self.inner
            .peers
            .lock()
            .expect("peer lock poisoned")
            .remove(&peer_id);
        self.inner
            .manager
            .close_tunnel(self.inner.vault_id, peer_id)
            .await
    }
}

impl<A, V, P> Clone for ScopedIrohTransport<A, V, P>
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
    P: TunnelAuthorizationProvider,
{
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<A, V, P> Transport for ScopedIrohTransport<A, V, P>
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
    P: TunnelAuthorizationProvider,
{
    type Error = Error;
    type PeerId = EndpointId;
    type Incoming = ScopedIrohIncoming;

    fn peers(&self) -> Vec<Self::PeerId> {
        Self::peers(self)
    }

    async fn send(&self, peer: Self::PeerId, data: Vec<u8>) -> Result<(), Self::Error> {
        if data.len() > MAX_FRAME_SIZE {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "file-system message is too large",
            ));
        }
        if !self.inner.manager.is_allowed(peer).await {
            self.disconnect(peer).await;
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "Iroh peer is not allowed",
            ));
        }
        let tunnel = self
            .inner
            .peers
            .lock()
            .expect("peer lock poisoned")
            .get(&peer)
            .cloned()
            .ok_or_else(|| Error::new(ErrorKind::NotConnected, "Iroh peer is not connected"))?;
        write_frame(&tunnel, &data).await
    }

    async fn broadcast(&self, data: Vec<u8>) -> Result<(), Self::Error> {
        if data.len() > MAX_FRAME_SIZE {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "file-system message is too large",
            ));
        }
        let peers = self
            .inner
            .peers
            .lock()
            .expect("peer lock poisoned")
            .iter()
            .map(|(peer_id, tunnel)| (*peer_id, tunnel.clone()))
            .collect::<Vec<_>>();
        for (peer_id, tunnel) in peers {
            if !self.inner.manager.is_allowed(peer_id).await {
                self.disconnect(peer_id).await;
                continue;
            }
            write_frame(&tunnel, &data).await?;
        }
        Ok(())
    }

    async fn subscribe(&self) -> Result<Self::Incoming, Self::Error> {
        self.inner
            .incoming
            .lock()
            .await
            .take()
            .ok_or_else(|| Error::new(ErrorKind::AlreadyExists, "transport subscribed twice"))
    }
}

async fn add_tunnel<A, V, P>(inner: &Arc<ScopedIrohTransportInner<A, V, P>>, tunnel: Tunnel)
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
    P: TunnelAuthorizationProvider,
{
    let peer_id = tunnel.remote_id();
    if inner
        .peers
        .lock()
        .expect("peer lock poisoned")
        .insert(peer_id, tunnel.clone())
        .is_some()
    {
        tunnel.close().await;
        return;
    }
    let _ = inner.peer_events.send(peer_id);
    let incoming_tx = inner.incoming_tx.clone();
    let task_inner = Arc::clone(inner);
    tokio::spawn(async move {
        let mut reader = tunnel.reader().await;
        loop {
            let mut length = [0_u8; 4];
            if reader.read_exact(&mut length).await.is_err() {
                break;
            }
            let length = usize::try_from(u32::from_be_bytes(length)).expect("u32 fits usize");
            if length > MAX_FRAME_SIZE {
                break;
            }
            let mut data = vec![0; length];
            if reader.read_exact(&mut data).await.is_err() {
                break;
            }
            if !task_inner.manager.is_allowed(peer_id).await {
                break;
            }
            if incoming_tx.send((peer_id, data)).await.is_err() {
                break;
            }
        }
        drop(reader);
        task_inner
            .peers
            .lock()
            .expect("peer lock poisoned")
            .remove(&peer_id);
        task_inner
            .manager
            .close_tunnel(task_inner.vault_id, peer_id)
            .await;
    });
}

async fn write_frame(tunnel: &Tunnel, data: &[u8]) -> Result<(), Error> {
    let length = u32::try_from(data.len())
        .map_err(|_| Error::new(ErrorKind::InvalidInput, "frame exceeds u32"))?;
    let mut writer = tunnel.writer().await;
    writer.write_all(&length.to_be_bytes()).await?;
    writer.write_all(data).await?;
    writer.flush().await
}
