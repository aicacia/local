use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};

use dashmap::DashMap;
use futures::future::join_all;
use iroh::{Endpoint, EndpointAddr, EndpointId, endpoint::Connection};
use iroh_tickets::endpoint::EndpointTicket;
use tokio::{
    select, spawn,
    sync::{
        Mutex, MutexGuard,
        mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    },
};
use tokio_util::sync::CancellationToken;

use crate::{peer::Peer, store::AllowedEndpointId};

pub const IRON_CHAIN_V1_ALPN: &[u8] = b"iron-chain/v1";

#[derive(Clone, Debug)]
pub enum ServerEvent {
    Connected(Peer),
    Disconnected(EndpointId),
}

struct ServerInner<A>
where
    A: AllowedEndpointId,
{
    endpoint: Endpoint,
    alpn: Vec<u8>,
    store: A,
    peers: DashMap<EndpointId, Peer>,
    closed_tx: UnboundedSender<EndpointId>,
    closed_rx: Mutex<UnboundedReceiver<EndpointId>>,
    event_tx: UnboundedSender<ServerEvent>,
    event_rx: Mutex<UnboundedReceiver<ServerEvent>>,
    cancellation_token: CancellationToken,
}

impl<A> ServerInner<A>
where
    A: AllowedEndpointId,
{
    fn new(endpoint: Endpoint, alpn: Vec<u8>, store: A) -> Self {
        let (event_tx, event_rx) = unbounded_channel();
        let (closed_tx, closed_rx) = unbounded_channel();

        Self {
            endpoint,
            alpn,
            store,
            peers: DashMap::new(),
            closed_tx,
            closed_rx: Mutex::new(closed_rx),
            event_tx,
            event_rx: Mutex::new(event_rx),
            cancellation_token: CancellationToken::new(),
        }
    }

    async fn connect(&self, endpoint: impl Into<EndpointAddr>) -> Result<(), Error> {
        let endpoint_addr = endpoint.into();

        tracing::info!("connecting to {:?}", endpoint_addr);

        let connection = self
            .endpoint
            .connect(endpoint_addr, &self.alpn)
            .await
            .map_err(Error::other)?;

        tracing::info!(
            "connected to {:?} with alpn {:?}",
            connection.remote_id(),
            self.alpn
        );

        self.internal_connect(connection, true).await
    }

    async fn internal_connect(&self, connection: Connection, outbound: bool) -> Result<(), Error> {
        let remote = connection.remote_id();
        tracing::info!("internal connect {remote} (outbound: {outbound})");

        if !self.store.allowed(remote).await {
            tracing::warn!("peer {remote} is not allowed");
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                format!("peer {remote} is not allowed"),
            ));
        }
        tracing::info!("peer {remote} is allowed");

        let (send, recv) = if outbound {
            connection.open_bi().await?
        } else {
            connection.accept_bi().await?
        };
        tracing::info!("bi-directional stream established with {remote}");

        let cancellation_token = CancellationToken::new();

        let peer = Peer::new(
            remote,
            send,
            recv,
            self.closed_tx.clone(),
            cancellation_token,
        );
        self.peers.insert(remote, peer.clone());

        self.event_tx
            .send(ServerEvent::Connected(peer))
            .map_err(|e| {
                Error::new(
                    ErrorKind::Other,
                    format!("error sending connected event: {e}"),
                )
            })?;

        Ok(())
    }

    fn try_get(&self, id: EndpointId) -> Option<Peer> {
        self.peers.get(&id).map(|entry| entry.value().clone())
    }

    async fn disconnect(&self, id: EndpointId) {
        if let Some((_, peer)) = self.peers.remove(&id) {
            peer.close().await;

            match self.event_tx.send(ServerEvent::Disconnected(id)) {
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("error sending disconnected event: {e}");
                }
            }
        }
    }

    async fn disconnect_all(&self) {
        let endpoint_ids = self
            .peers
            .iter()
            .map(|entry| self.disconnect(entry.id().clone()))
            .collect::<Vec<_>>();

        let _ = join_all(endpoint_ids).await;
    }

    async fn listen(self: Arc<Self>) {
        let cancellation_token = self.cancellation_token.clone();
        let mut closed_rx = self.closed_rx.lock().await;

        loop {
            select! {
                _ = cancellation_token.cancelled() => break,
                incoming = self.endpoint.accept() => {
                    let Some(connecting) = incoming else { break };
                    let this = Arc::clone(&self);

                    tracing::info!("incoming connection");

                    spawn(async move {
                        tracing::info!("waiting for handshake");

                        let connection = match connecting.await {
                            Ok(c) => c,
                            Err(e) => {
                                tracing::warn!("error accepting connection: {e}");
                                return;
                            }
                        };
                        tracing::info!(
                            "handshake succeeded from {:?}",
                            connection.remote_id()
                        );

                        match this.internal_connect(connection, false).await {
                            Ok(_) => {},
                            Err(e) => {
                                tracing::warn!("error connecting to peer: {e}");
                            }
                        }
                    });
                }
                Some(id) = closed_rx.recv() => {
                    self.disconnect(id).await;
                }
            }
        }
    }

    async fn close(&self) {
        self.endpoint.close().await;
        self.disconnect_all().await;
    }
}

#[derive(Clone)]
pub struct Server<A>
where
    A: AllowedEndpointId,
{
    inner: Arc<ServerInner<A>>,
}

impl<A> Server<A>
where
    A: AllowedEndpointId,
{
    pub fn new(endpoint: Endpoint, store: A) -> Self {
        let inner = ServerInner::new(endpoint, IRON_CHAIN_V1_ALPN.to_vec(), store);

        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.inner.endpoint
    }

    pub fn ticket(&self) -> EndpointTicket {
        EndpointTicket::new(self.inner.endpoint.addr())
    }

    pub async fn connect(&self, endpoint: impl Into<EndpointAddr>) -> Result<(), Error> {
        self.inner.connect(endpoint).await
    }

    pub async fn event_receiver(&self) -> MutexGuard<'_, UnboundedReceiver<ServerEvent>> {
        self.inner.event_rx.lock().await
    }

    pub fn try_get(&self, id: EndpointId) -> Option<Peer> {
        self.inner.try_get(id)
    }

    pub fn store(&self) -> &A {
        &self.inner.store
    }

    pub async fn listen(&self) {
        let inner = Arc::clone(&self.inner);
        inner.listen().await;
    }

    pub async fn close(&self) {
        self.inner.close().await;
    }
}
