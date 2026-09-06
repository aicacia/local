use std::{
    collections::BTreeMap,
    io::{Error, ErrorKind},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use file_system::Transport;
use iroh::{EndpointAddr, EndpointId};
use iroh_chain::{AllowedEndpointId, Tunnel, TunnelAuthorizer, TunnelManager, VaultId};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Mutex as AsyncMutex, broadcast, mpsc},
    task::JoinHandle,
};

const INCOMING_CAPACITY: usize = 64;
const MAX_FRAME_SIZE: usize = 1024 * 1024;

type Message = (EndpointId, Vec<u8>);

pub struct ScopedIrohIncoming(mpsc::Receiver<Message>);

impl futures_core::Stream for ScopedIrohIncoming {
    type Item = Message;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.poll_recv(context)
    }
}

pub struct ScopedIrohTransport<A, V>
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
{
    inner: Arc<ScopedIrohTransportInner<A, V>>,
    task: JoinHandle<()>,
}

struct ScopedIrohTransportInner<A, V>
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
{
    manager: TunnelManager<A, V>,
    vault_id: VaultId,
    authorization: Vec<u8>,
    peers: Mutex<BTreeMap<EndpointId, Tunnel>>,
    incoming: AsyncMutex<Option<ScopedIrohIncoming>>,
    incoming_tx: mpsc::Sender<Message>,
    peer_events: broadcast::Sender<EndpointId>,
}

impl<A, V> ScopedIrohTransport<A, V>
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
{
    pub fn new(manager: TunnelManager<A, V>, vault_id: VaultId, authorization: Vec<u8>) -> Self {
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
        let task = tokio::spawn(async move {
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
        Self { inner, task }
    }

    pub async fn connect(&self, endpoint: impl Into<EndpointAddr>) -> Result<EndpointId, Error> {
        let tunnel = self
            .inner
            .manager
            .connect(self.inner.vault_id, endpoint, &self.inner.authorization)
            .await?;
        let peer_id = tunnel.remote_id();
        add_tunnel(&self.inner, tunnel).await;
        Ok(peer_id)
    }

    pub fn subscribe_peers(&self) -> broadcast::Receiver<EndpointId> {
        self.inner.peer_events.subscribe()
    }
}

impl<A, V> Drop for ScopedIrohTransport<A, V>
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
{
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl<A, V> Transport for ScopedIrohTransport<A, V>
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
{
    type Error = Error;
    type PeerId = EndpointId;
    type Incoming = ScopedIrohIncoming;

    async fn send(&self, peer: Self::PeerId, data: Vec<u8>) -> Result<(), Self::Error> {
        if data.len() > MAX_FRAME_SIZE {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "file-system message is too large",
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
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for peer in peers {
            write_frame(&peer, &data).await?;
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

async fn add_tunnel<A, V>(inner: &Arc<ScopedIrohTransportInner<A, V>>, tunnel: Tunnel)
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
{
    let peer_id = tunnel.remote_id();
    if inner
        .peers
        .lock()
        .expect("peer lock poisoned")
        .insert(peer_id, tunnel.clone())
        .is_some()
    {
        return;
    }
    let _ = inner.peer_events.send(peer_id);
    let incoming_tx = inner.incoming_tx.clone();
    tokio::spawn(async move {
        let mut reader = tunnel.reader().await;
        loop {
            let mut length = [0_u8; 4];
            if reader.read_exact(&mut length).await.is_err() {
                return;
            }
            let length = usize::try_from(u32::from_be_bytes(length)).expect("u32 fits usize");
            if length > MAX_FRAME_SIZE {
                return;
            }
            let mut data = vec![0; length];
            if reader.read_exact(&mut data).await.is_err() {
                return;
            }
            if incoming_tx.send((peer_id, data)).await.is_err() {
                return;
            }
        }
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
