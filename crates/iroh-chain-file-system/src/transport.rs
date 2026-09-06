use std::{
    collections::BTreeMap,
    io::{Error, ErrorKind},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use file_system::Transport;
use iroh::EndpointId;
use iroh_chain::{AllowedEndpointId, Peer, Server, ServerEvent};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Mutex as AsyncMutex, broadcast, mpsc},
    task::JoinHandle,
};

const MAX_FRAME_SIZE: usize = 1024 * 1024;
const INCOMING_CAPACITY: usize = 64;

type Message = (EndpointId, Vec<u8>);

pub struct IrohIncoming(mpsc::Receiver<Message>);

impl futures_core::Stream for IrohIncoming {
    type Item = Message;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.poll_recv(context)
    }
}

pub struct IrohTransport<A: AllowedEndpointId> {
    inner: Arc<IrohTransportInner<A>>,
    task: JoinHandle<()>,
}

struct IrohTransportInner<A: AllowedEndpointId> {
    server: Server<A>,
    peers: Mutex<BTreeMap<EndpointId, Peer>>,
    incoming: AsyncMutex<Option<IrohIncoming>>,
    incoming_tx: mpsc::Sender<Message>,
    peer_events: broadcast::Sender<EndpointId>,
}

impl<A: AllowedEndpointId> IrohTransport<A> {
    pub fn new(server: Server<A>) -> Self {
        let (incoming_tx, incoming_rx) = mpsc::channel(INCOMING_CAPACITY);
        let (peer_events, _) = broadcast::channel(INCOMING_CAPACITY);
        let inner = Arc::new(IrohTransportInner {
            server: server.clone(),
            peers: Mutex::new(BTreeMap::new()),
            incoming: AsyncMutex::new(Some(IrohIncoming(incoming_rx))),
            incoming_tx,
            peer_events,
        });
        let mut events = server.subscribe_events();
        let task_inner = Arc::clone(&inner);
        let task = tokio::spawn(async move {
            for peer in server.peers() {
                add_peer(&task_inner, peer).await;
            }
            loop {
                match events.recv().await {
                    Ok(ServerEvent::Connected(peer)) => add_peer(&task_inner, peer).await,
                    Ok(ServerEvent::Disconnected(id)) => {
                        task_inner
                            .peers
                            .lock()
                            .expect("peer lock poisoned")
                            .remove(&id);
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });
        Self { inner, task }
    }

    pub fn subscribe_peers(&self) -> broadcast::Receiver<EndpointId> {
        self.inner.peer_events.subscribe()
    }
}

impl<A: AllowedEndpointId> Drop for IrohTransport<A> {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl<A: AllowedEndpointId> Transport for IrohTransport<A> {
    type Error = Error;
    type PeerId = EndpointId;
    type Incoming = IrohIncoming;

    async fn send(&self, peer: Self::PeerId, data: Vec<u8>) -> Result<(), Self::Error> {
        if data.len() > MAX_FRAME_SIZE {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "file-system message is too large",
            ));
        }
        let peer = self
            .inner
            .peers
            .lock()
            .expect("peer lock poisoned")
            .get(&peer)
            .cloned()
            .ok_or_else(|| Error::new(ErrorKind::NotConnected, "Iroh peer is not connected"))?;
        write_frame(&peer, &data).await
    }

    async fn broadcast(&self, data: Vec<u8>) -> Result<(), Self::Error> {
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

async fn add_peer<A: AllowedEndpointId>(inner: &Arc<IrohTransportInner<A>>, peer: Peer) {
    let id = peer.id();
    inner
        .peers
        .lock()
        .expect("peer lock poisoned")
        .insert(id, peer.clone());
    let _ = inner.peer_events.send(id);
    let incoming_tx = inner.incoming_tx.clone();
    let server = inner.server.clone();
    tokio::spawn(async move {
        if write_frame(&peer, &[]).await.is_err() {
            server.disconnect_peer(&peer).await;
            return;
        }
        let mut reader = peer.reader().await;
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
            if !data.is_empty() && incoming_tx.send((id, data)).await.is_err() {
                break;
            }
        }
        server.disconnect_peer(&peer).await;
    });
}

async fn write_frame(peer: &Peer, data: &[u8]) -> Result<(), Error> {
    let length = u32::try_from(data.len())
        .map_err(|_| Error::new(ErrorKind::InvalidInput, "frame exceeds u32"))?;
    let mut writer = peer.writer().await;
    writer.write_all(&length.to_be_bytes()).await?;
    writer.write_all(data).await?;
    writer.flush().await
}
