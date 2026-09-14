use alloc::sync::Arc;
use core::{future::poll_fn, pin::Pin};

use futures_core::Stream;
use tokio::sync::{Mutex, mpsc};

use crate::{
    PeerCodec, Transport,
    file_system::{FileSystemError, FileSystemState, SyncRequest},
};

pub(crate) struct SyncSession {
    task: tokio::task::JoinHandle<()>,
}

impl SyncSession {
    pub(crate) fn start<S, C, T>(
        state: Arc<Mutex<FileSystemState<S, C>>>,
        transport: Arc<T>,
        incoming: Pin<Box<T::Incoming>>,
        mut requests: mpsc::Receiver<SyncRequest<C::PeerId>>,
    ) -> Self
    where
        S: crate::Storage + Send + Sync + 'static,
        S::Error: Send + 'static,
        C: PeerCodec + Send + 'static,
        C::PeerId: Send + 'static,
        T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
        T::Incoming: 'static,
    {
        let task_state = Arc::clone(&state);
        let task_transport = Arc::clone(&transport);
        let task = tokio::spawn(async move {
            let mut incoming = incoming;
            loop {
                tokio::select! {
                    request = requests.recv() => match request {
                        Some(SyncRequest::Broadcast(data)) => {
                            let _ = task_transport.broadcast(data).await;
                        }
                        Some(SyncRequest::Send { peer, data }) => {
                            let _ = task_transport.send(peer, data).await;
                        }
                        Some(SyncRequest::Upload { peer, data, id }) => {
                            if task_transport.send(peer, data).await.is_err()
                                && let Some(pending) = task_state.lock().await.pending_uploads.remove(&id)
                            {
                                let _ = pending.sender.send(Err(FileSystemError::InvalidMetadata));
                            }
                        }
                        None => return,
                    },
                    message = poll_fn(|context| incoming.as_mut().poll_next(context)) => match message {
                        Some((peer, data)) => {
                            let _ = task_state.lock().await.receive(peer, data).await;
                        }
                        None => return,
                    }
                }
            }
        });
        Self { task }
    }

    pub(crate) fn abort(&self) {
        self.task.abort();
    }
}
