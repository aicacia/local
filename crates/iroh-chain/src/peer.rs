use std::{
    io::Error,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use iroh::EndpointId;
use noq::{RecvStream, SendStream};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::{Mutex, MutexGuard, mpsc::UnboundedSender},
};
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
struct PeerInner {
    id: EndpointId,
    session: u64,
    send: Mutex<SendStream>,
    recv: Mutex<RecvStream>,
    closed_tx: Mutex<Option<UnboundedSender<(EndpointId, u64)>>>,
    cancellation_token: CancellationToken,
}

#[derive(Clone, Debug)]
pub struct Peer {
    inner: Arc<PeerInner>,
}

impl Peer {
    pub(crate) fn new(
        id: EndpointId,
        session: u64,
        send: SendStream,
        recv: RecvStream,
        closed_tx: UnboundedSender<(EndpointId, u64)>,
        cancellation_token: CancellationToken,
    ) -> Self {
        Self {
            inner: Arc::new(PeerInner {
                id,
                session,
                send: Mutex::new(send),
                recv: Mutex::new(recv),
                closed_tx: Mutex::new(Some(closed_tx)),
                cancellation_token,
            }),
        }
    }

    pub fn id(&self) -> EndpointId {
        self.inner.id
    }

    pub(crate) fn session(&self) -> u64 {
        self.inner.session
    }

    pub(crate) async fn close(&self) {
        if let Some(closed_tx) = self.inner.closed_tx.lock().await.take() {
            self.inner.cancellation_token.cancel();
            match closed_tx.send((self.inner.id, self.inner.session)) {
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(
                        "failed to send closed event for peer {}: {}",
                        self.inner.id,
                        e
                    );
                }
            }
        }
    }

    pub async fn send(&self) -> MutexGuard<'_, noq::SendStream> {
        self.inner.send.lock().await
    }

    pub async fn recv(&self) -> MutexGuard<'_, noq::RecvStream> {
        self.inner.recv.lock().await
    }

    pub async fn reader(&self) -> PeerReader<'_> {
        PeerReader {
            guard: self.inner.recv.lock().await,
        }
    }

    pub async fn writer(&self) -> PeerWriter<'_> {
        PeerWriter {
            guard: self.inner.send.lock().await,
        }
    }
}

pub struct PeerReader<'a> {
    guard: MutexGuard<'a, RecvStream>,
}

impl AsyncRead for PeerReader<'_> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), Error>> {
        Pin::new(&mut *self.guard).poll_read(cx, buf)
    }
}

pub struct PeerWriter<'a> {
    guard: MutexGuard<'a, SendStream>,
}

impl AsyncWrite for PeerWriter<'_> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        Pin::new(&mut *self.guard)
            .poll_write(cx, buf)
            .map_err(Error::other)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Pin::new(&mut *self.guard).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Pin::new(&mut *self.guard).poll_shutdown(cx)
    }
}
