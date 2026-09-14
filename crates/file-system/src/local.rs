use alloc::{string::String, sync::Arc, vec::Vec};
use core::{
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll},
};

use futures_core::Stream;
use tokio::sync::{Mutex, mpsc};

use crate::{
    FileEntry, FileSystemError, PeerCodec, Storage, Transport,
    local_state::{
        ContentRecoveryReport, LocalFileSystemState, MetadataRecoveryReport, OutboundRecoveryReport,
    },
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LocalPeer;

#[derive(Debug)]
pub struct LocalPeerCodec;

impl PeerCodec for LocalPeerCodec {
    type Error = Infallible;
    type PeerId = LocalPeer;

    fn encode(_: &Self::PeerId) -> Vec<u8> {
        Vec::new()
    }

    fn decode(_: &[u8]) -> Result<Self::PeerId, Self::Error> {
        Ok(LocalPeer)
    }
}

#[derive(Debug)]
pub struct LocalIncoming(mpsc::UnboundedReceiver<(LocalPeer, Vec<u8>)>);

impl Stream for LocalIncoming {
    type Item = (LocalPeer, Vec<u8>);

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.poll_recv(context)
    }
}

#[derive(Debug)]
pub struct LocalTransport {
    _sender: mpsc::UnboundedSender<(LocalPeer, Vec<u8>)>,
    incoming: Mutex<Option<LocalIncoming>>,
}

impl LocalTransport {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        Self {
            _sender: sender,
            incoming: Mutex::new(Some(LocalIncoming(receiver))),
        }
    }
}

impl Default for LocalTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for LocalTransport {
    type Error = Infallible;
    type PeerId = LocalPeer;
    type Incoming = LocalIncoming;

    async fn send(&self, _: Self::PeerId, _: Vec<u8>) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn broadcast(&self, _: Vec<u8>) -> Result<(), Self::Error> {
        Ok(())
    }

    fn peers(&self) -> Vec<Self::PeerId> {
        Vec::new()
    }

    async fn subscribe(&self) -> Result<Self::Incoming, Self::Error> {
        Ok(self
            .incoming
            .lock()
            .await
            .take()
            .expect("local transport subscribed twice"))
    }
}

#[derive(Clone)]
pub struct LocalFileSystem<S: Storage> {
    pub(crate) state: Arc<Mutex<LocalFileSystemState<S, LocalPeerCodec>>>,
}

impl<S: Storage> LocalFileSystem<S> {
    pub async fn open_local(storage: S) -> Result<Self, FileSystemError<S::Error>> {
        let state = LocalFileSystemState::open(storage, LocalPeer).await?;
        Ok(Self {
            state: Arc::new(Mutex::new(state)),
        })
    }

    pub async fn with_storage<R>(&self, f: impl FnOnce(&S) -> R) -> R {
        let state = self.state.lock().await;
        f(state.storage())
    }

    pub async fn with_storage_mut<R>(&self, f: impl FnOnce(&mut S) -> R) -> R {
        let mut state = self.state.lock().await;
        f(state.storage_mut())
    }

    pub async fn read(&self, path: &str) -> Result<Vec<u8>, FileSystemError<S::Error>> {
        let state = self.state.lock().await;
        LocalFileSystemState::<S, LocalPeerCodec>::read(&state, path).await
    }

    pub async fn write(
        &self,
        path: &str,
        content: &[u8],
    ) -> Result<FileEntry<LocalPeer>, FileSystemError<S::Error>> {
        let mut state = self.state.lock().await;
        let (entry, _) =
            LocalFileSystemState::<S, LocalPeerCodec>::write(&mut state, path, content).await?;
        Ok(entry)
    }

    pub async fn append(
        &self,
        path: &str,
        content: &[u8],
    ) -> Result<FileEntry<LocalPeer>, FileSystemError<S::Error>> {
        let mut state = self.state.lock().await;
        let (entry, _) =
            LocalFileSystemState::<S, LocalPeerCodec>::append(&mut state, path, content).await?;
        Ok(entry)
    }

    pub async fn delete(&self, path: &str) -> Result<(), FileSystemError<S::Error>> {
        let mut state = self.state.lock().await;
        LocalFileSystemState::<S, LocalPeerCodec>::delete(&mut state, path).await?;
        Ok(())
    }

    pub async fn rename(&self, from: &str, to: &str) -> Result<(), FileSystemError<S::Error>> {
        let mut state = self.state.lock().await;
        LocalFileSystemState::<S, LocalPeerCodec>::rename(&mut state, from, to).await?;
        Ok(())
    }

    pub async fn entry(
        &self,
        path: &str,
    ) -> Result<FileEntry<LocalPeer>, FileSystemError<S::Error>> {
        let state = self.state.lock().await;
        LocalFileSystemState::<S, LocalPeerCodec>::entry(&state, path)
    }

    pub async fn list(
        &self,
        folder: &str,
    ) -> Result<Vec<FileEntry<LocalPeer>>, FileSystemError<S::Error>> {
        let state = self.state.lock().await;
        LocalFileSystemState::<S, LocalPeerCodec>::list(&state, folder)
    }

    pub async fn paths(&self) -> Result<Vec<String>, FileSystemError<S::Error>> {
        let state = self.state.lock().await;
        LocalFileSystemState::<S, LocalPeerCodec>::paths(&state)
    }

    pub async fn recover_metadata(
        &self,
    ) -> Result<MetadataRecoveryReport, FileSystemError<S::Error>> {
        let mut state = self.state.lock().await;
        LocalFileSystemState::<S, LocalPeerCodec>::recover_metadata(&mut state).await
    }

    pub async fn recover_content(
        &self,
    ) -> Result<ContentRecoveryReport, FileSystemError<S::Error>> {
        let mut state = self.state.lock().await;
        LocalFileSystemState::<S, LocalPeerCodec>::recover_content(&mut state).await
    }

    pub async fn recover_outbound(
        &self,
    ) -> Result<OutboundRecoveryReport, FileSystemError<S::Error>> {
        let mut state = self.state.lock().await;
        LocalFileSystemState::<S, LocalPeerCodec>::recover_outbound(&mut state).await
    }

    pub async fn flush_outbound(&self) -> Result<(), FileSystemError<S::Error>> {
        let mut state = self.state.lock().await;
        LocalFileSystemState::<S, LocalPeerCodec>::flush_outbound(&mut state).await
    }
}
