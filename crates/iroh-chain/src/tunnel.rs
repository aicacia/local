use std::{
    collections::BTreeMap,
    io::{Error, ErrorKind},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use iroh::{Endpoint, EndpointAddr, EndpointId, endpoint::Connection};
use noq::{RecvStream, SendStream, VarInt};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf},
    sync::{Mutex, MutexGuard, broadcast},
};

use crate::AllowedEndpointId;

pub const TUNNEL_ALPN: &[u8] = b"lidp-tunnel/1";
const PROTOCOL_VERSION: u8 = 1;
const MAX_AUTHORIZATION_LENGTH: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct VaultId([u8; 32]);

impl VaultId {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

pub trait TunnelAuthorizer: Send + Sync + 'static {
    fn authorize(
        &self,
        vault_id: VaultId,
        local_id: EndpointId,
        remote_id: EndpointId,
        authorization: &[u8],
    ) -> impl Future<Output = bool> + Send;
}

#[derive(Clone, Debug)]
pub struct Tunnel {
    inner: Arc<TunnelInner>,
}

#[derive(Debug)]
struct TunnelInner {
    remote_id: EndpointId,
    send: Mutex<SendStream>,
    recv: Mutex<RecvStream>,
}

impl Tunnel {
    fn new(remote_id: EndpointId, send: SendStream, recv: RecvStream) -> Self {
        Self {
            inner: Arc::new(TunnelInner {
                remote_id,
                send: Mutex::new(send),
                recv: Mutex::new(recv),
            }),
        }
    }

    pub fn remote_id(&self) -> EndpointId {
        self.inner.remote_id
    }

    pub async fn reader(&self) -> TunnelReader<'_> {
        TunnelReader {
            guard: self.inner.recv.lock().await,
        }
    }

    pub async fn writer(&self) -> TunnelWriter<'_> {
        TunnelWriter {
            guard: self.inner.send.lock().await,
        }
    }

    pub async fn close(&self) {
        let _ = self.inner.send.lock().await.reset(VarInt::from_u32(1));
        let _ = self.inner.recv.lock().await.stop(VarInt::from_u32(1));
    }
}

pub struct TunnelReader<'a> {
    guard: MutexGuard<'a, RecvStream>,
}

impl AsyncRead for TunnelReader<'_> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), Error>> {
        Pin::new(&mut *self.guard).poll_read(context, buffer)
    }
}

pub struct TunnelWriter<'a> {
    guard: MutexGuard<'a, SendStream>,
}

impl AsyncWrite for TunnelWriter<'_> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<Result<usize, Error>> {
        Pin::new(&mut *self.guard)
            .poll_write(context, buffer)
            .map_err(Error::other)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Pin::new(&mut *self.guard).poll_flush(context)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Result<(), Error>> {
        Pin::new(&mut *self.guard).poll_shutdown(context)
    }
}

#[derive(Clone, Debug)]
pub struct TunnelEvent {
    pub vault_id: VaultId,
    pub tunnel: Tunnel,
}

struct TunnelManagerInner<A, V>
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
{
    endpoint: Endpoint,
    allowed: A,
    authorizer: V,
    tunnels: Mutex<BTreeMap<(EndpointId, VaultId), Tunnel>>,
    events: broadcast::Sender<TunnelEvent>,
}

pub struct TunnelManager<A, V>
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
{
    inner: Arc<TunnelManagerInner<A, V>>,
}

impl<A, V> Clone for TunnelManager<A, V>
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
{
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<A, V> TunnelManager<A, V>
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
{
    pub fn new(endpoint: Endpoint, allowed: A, authorizer: V) -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            inner: Arc::new(TunnelManagerInner {
                endpoint,
                allowed,
                authorizer,
                tunnels: Mutex::new(BTreeMap::new()),
                events,
            }),
        }
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.inner.endpoint
    }

    pub fn subscribe(&self) -> broadcast::Receiver<TunnelEvent> {
        self.inner.events.subscribe()
    }

    pub async fn connect(
        &self,
        vault_id: VaultId,
        endpoint: impl Into<EndpointAddr>,
        authorization: &[u8],
    ) -> Result<Tunnel, Error> {
        let connection = self
            .inner
            .endpoint
            .connect(endpoint, TUNNEL_ALPN)
            .await
            .map_err(Error::other)?;
        let remote_id = connection.remote_id();
        if !self.inner.allowed.allowed(remote_id).await {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "peer is not allowed",
            ));
        }
        let (mut send, mut recv) = connection.open_bi().await?;
        write_handshake(&mut send, vault_id, self.inner.endpoint.id(), authorization).await?;
        if recv.read_u8().await? == 0 {
            return Err(Error::new(
                ErrorKind::PermissionDenied,
                "tunnel was rejected",
            ));
        }
        self.insert(vault_id, Tunnel::new(remote_id, send, recv), false)
            .await
    }

    pub async fn listen(&self) {
        loop {
            let Some(connecting) = self.inner.endpoint.accept().await else {
                return;
            };
            let manager = self.clone();
            tokio::spawn(async move {
                let Ok(connection) = connecting.await else {
                    return;
                };
                manager.accept_connection(connection).await;
            });
        }
    }

    pub async fn close(&self) {
        self.inner.endpoint.close().await;
        let tunnels = std::mem::take(&mut *self.inner.tunnels.lock().await);
        for tunnel in tunnels.into_values() {
            tunnel.close().await;
        }
    }

    pub async fn close_tunnel(&self, vault_id: VaultId, remote_id: EndpointId) -> bool {
        let tunnel = self
            .inner
            .tunnels
            .lock()
            .await
            .remove(&(remote_id, vault_id));
        if let Some(tunnel) = tunnel {
            tunnel.close().await;
            true
        } else {
            false
        }
    }

    async fn accept_connection(&self, connection: Connection) {
        let remote_id = connection.remote_id();
        if !self.inner.allowed.allowed(remote_id).await {
            return;
        }
        while let Ok((mut send, mut recv)) = connection.accept_bi().await {
            if !self.inner.allowed.allowed(remote_id).await {
                let _ = send.write_u8(0).await;
                let _ = send.flush().await;
                continue;
            }
            let Ok((vault_id, initiating_id, authorization)) = read_handshake(&mut recv).await
            else {
                return;
            };
            let accepted = initiating_id == remote_id
                && self
                    .inner
                    .authorizer
                    .authorize(
                        vault_id,
                        self.inner.endpoint.id(),
                        remote_id,
                        &authorization,
                    )
                    .await;
            if send.write_u8(u8::from(accepted)).await.is_err() || send.flush().await.is_err() {
                return;
            }
            if !accepted {
                continue;
            }
            if self
                .insert(vault_id, Tunnel::new(remote_id, send, recv), true)
                .await
                .is_err()
            {
                return;
            }
        }
    }

    async fn insert(
        &self,
        vault_id: VaultId,
        tunnel: Tunnel,
        notify: bool,
    ) -> Result<Tunnel, Error> {
        let key = (tunnel.remote_id(), vault_id);
        let mut tunnels = self.inner.tunnels.lock().await;
        if tunnels.contains_key(&key) {
            return Err(Error::new(
                ErrorKind::AlreadyExists,
                "tunnel already exists",
            ));
        }
        tunnels.insert(key, tunnel.clone());
        drop(tunnels);
        if notify {
            let _ = self.inner.events.send(TunnelEvent {
                vault_id,
                tunnel: tunnel.clone(),
            });
        }
        Ok(tunnel)
    }
}

async fn write_handshake(
    send: &mut SendStream,
    vault_id: VaultId,
    initiating_id: EndpointId,
    authorization: &[u8],
) -> Result<(), Error> {
    let authorization_length = u16::try_from(authorization.len())
        .map_err(|_| Error::new(ErrorKind::InvalidInput, "tunnel authorization is too large"))?;
    if authorization.len() > MAX_AUTHORIZATION_LENGTH {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "tunnel authorization is too large",
        ));
    }
    send.write_u8(PROTOCOL_VERSION).await?;
    send.write_all(vault_id.as_bytes()).await?;
    send.write_all(initiating_id.as_bytes()).await?;
    send.write_u16(authorization_length).await?;
    send.write_all(authorization).await?;
    send.flush().await
}

async fn read_handshake(recv: &mut RecvStream) -> Result<(VaultId, EndpointId, Vec<u8>), Error> {
    if recv.read_u8().await? != PROTOCOL_VERSION {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "unsupported tunnel protocol",
        ));
    }
    let mut vault_id = [0_u8; 32];
    recv.read_exact(&mut vault_id).await.map_err(Error::other)?;
    let mut initiating_id = [0_u8; 32];
    recv.read_exact(&mut initiating_id)
        .await
        .map_err(Error::other)?;
    let initiating_id = EndpointId::from_bytes(&initiating_id).map_err(Error::other)?;
    let length = usize::from(recv.read_u16().await?);
    if length > MAX_AUTHORIZATION_LENGTH {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "tunnel authorization is too large",
        ));
    }
    let mut authorization = vec![0; length];
    recv.read_exact(&mut authorization)
        .await
        .map_err(Error::other)?;
    Ok((VaultId::new(vault_id), initiating_id, authorization))
}
