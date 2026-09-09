use std::{
    collections::BTreeMap,
    convert::Infallible,
    future::Future,
    io,
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use file_system::{FileSystem, NativeStorage, PeerCodec, Transport};
use futures_core::Stream;
use storage_model::StorageNamespace;
use tokio::sync::{Mutex, mpsc};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct StorageNamespaceId {
    user_sub: String,
    application_id: i64,
}

pub type ScopedFileSystem<C, T> = FileSystem<NativeStorage, C, T>;
pub type LocalScopedFileSystem = ScopedFileSystem<LocalPeerCodec, LocalTransport>;
pub type LocalScopedFileSystemRuntime<S> =
    ScopedFileSystemRuntime<LocalPeerCodec, LocalTransport, LocalTransportFactory, S>;

pub trait ScopedTransportFactory<C, T, S>: Send + Sync + 'static
where
    C: PeerCodec,
    T: Transport<PeerId = C::PeerId>,
{
    fn create(&self, scope: &S) -> Result<T, String>;

    fn synchronize(
        &self,
        scope: S,
        file_system: Arc<ScopedFileSystem<C, T>>,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>>;
}

pub struct ScopedFileSystemRuntime<C, T, F, S>
where
    C: PeerCodec,
    T: Transport<PeerId = C::PeerId>,
    F: ScopedTransportFactory<C, T, S>,
{
    root: PathBuf,
    local_peer: C::PeerId,
    transport_factory: F,
    file_systems: Mutex<BTreeMap<StorageNamespaceId, Arc<ScopedFileSystem<C, T>>>>,
    _scope: core::marker::PhantomData<fn(S)>,
}

impl<C, T, F, S> ScopedFileSystemRuntime<C, T, F, S>
where
    C: PeerCodec + Send + Sync + 'static,
    C::Error: Send + 'static,
    C::PeerId: Clone + Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Error: std::fmt::Display,
    T::Incoming: Send + 'static,
    F: ScopedTransportFactory<C, T, S>,
    S: StorageNamespace + Clone + Send + Sync + 'static,
{
    pub fn new(root: PathBuf, local_peer: C::PeerId, transport_factory: F) -> io::Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            local_peer,
            transport_factory,
            file_systems: Mutex::new(BTreeMap::new()),
            _scope: core::marker::PhantomData,
        })
    }

    pub async fn open(&self, scope: &S) -> Result<Arc<ScopedFileSystem<C, T>>, String> {
        let id = storage_namespace_id(scope)?;
        let mut file_systems = self.file_systems.lock().await;
        if let Some(file_system) = file_systems.get(&id) {
            let file_system = Arc::clone(file_system);
            drop(file_systems);
            self.transport_factory
                .synchronize(scope.clone(), Arc::clone(&file_system))
                .await?;
            return Ok(file_system);
        }
        let storage = NativeStorage::new(
            self.root
                .join("vaults")
                .join(&id.user_sub)
                .join(id.application_id.to_string()),
        )
        .map_err(|error| error.to_string())?;
        let file_system = Arc::new(
            FileSystem::new(
                storage,
                self.local_peer.clone(),
                self.transport_factory.create(scope)?,
            )
            .await
            .map_err(|error| error.to_string())?,
        );
        file_systems.insert(id, Arc::clone(&file_system));
        drop(file_systems);
        self.transport_factory
            .synchronize(scope.clone(), Arc::clone(&file_system))
            .await?;
        Ok(file_system)
    }
}

fn storage_namespace_id(scope: &impl StorageNamespace) -> Result<StorageNamespaceId, String> {
    let user_sub = scope.user_sub();
    let valid = |value: &str| {
        !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    };
    if !valid(user_sub) {
        return Err("invalid user subject".to_string());
    }
    if scope.application_id() <= 0 {
        return Err("invalid application id".to_string());
    }

    Ok(StorageNamespaceId {
        user_sub: user_sub.to_owned(),
        application_id: scope.application_id(),
    })
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub struct LocalPeer;

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

pub struct LocalIncoming(mpsc::UnboundedReceiver<(LocalPeer, Vec<u8>)>);

impl Stream for LocalIncoming {
    type Item = (LocalPeer, Vec<u8>);

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.poll_recv(context)
    }
}

pub struct LocalTransport {
    _sender: mpsc::UnboundedSender<(LocalPeer, Vec<u8>)>,
    incoming: Mutex<Option<LocalIncoming>>,
}

#[derive(Clone, Copy, Default)]
pub struct LocalTransportFactory;

impl<S: StorageNamespace> ScopedTransportFactory<LocalPeerCodec, LocalTransport, S>
    for LocalTransportFactory
{
    fn create(&self, _: &S) -> Result<LocalTransport, String> {
        Ok(LocalTransport::new())
    }

    fn synchronize(
        &self,
        _: S,
        _: Arc<LocalScopedFileSystem>,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
}

impl LocalTransport {
    fn new() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        Self {
            _sender: sender,
            incoming: Mutex::new(Some(LocalIncoming(receiver))),
        }
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

    async fn subscribe(&self) -> Result<Self::Incoming, Self::Error> {
        Ok(self
            .incoming
            .lock()
            .await
            .take()
            .expect("local transport subscribed twice"))
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use super::{LocalPeer, LocalScopedFileSystemRuntime, LocalTransportFactory, StorageNamespace};

    #[derive(Clone)]
    struct Namespace;

    impl StorageNamespace for Namespace {
        fn user_sub(&self) -> &str {
            "user"
        }

        fn application_id(&self) -> i64 {
            1
        }
    }

    #[tokio::test]
    async fn persists_scoped_native_storage() {
        let root = env::temp_dir().join(format!("scoped-file-system-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let namespace = Namespace;
        let runtime =
            LocalScopedFileSystemRuntime::new(root.clone(), LocalPeer, LocalTransportFactory)
                .unwrap();
        runtime
            .open(&namespace)
            .await
            .unwrap()
            .write("notes/today.txt", b"hello")
            .await
            .unwrap();
        drop(runtime);

        let runtime =
            LocalScopedFileSystemRuntime::new(root.clone(), LocalPeer, LocalTransportFactory)
                .unwrap();
        assert_eq!(
            runtime
                .open(&namespace)
                .await
                .unwrap()
                .read("notes/today.txt")
                .await
                .unwrap(),
            b"hello"
        );
        let _ = fs::remove_dir_all(root);
    }
}
