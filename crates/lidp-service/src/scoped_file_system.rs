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

use file_system::{EncryptedStorage, FileSystem, NativeStorage, PeerCodec, Transport};
use futures_core::Stream;
use tokio::sync::{Mutex, mpsc};

use crate::{repo::RawKeyringRepo, storage_session::StorageScope};

const VAULT_KEY_NAME: &str = "vault-key";

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct StorageScopeId {
    user_sub: String,
    application_id: i64,
}

pub type ScopedFileSystem<C, T> = FileSystem<EncryptedStorage<NativeStorage>, C, T>;
pub type LocalScopedFileSystem = ScopedFileSystem<LocalPeerCodec, LocalTransport>;
pub type LocalScopedFileSystemRuntime =
    ScopedFileSystemRuntime<LocalPeerCodec, LocalTransport, LocalTransportFactory>;

pub trait ScopedTransportFactory<C, T>: Send + Sync + 'static
where
    C: PeerCodec,
    T: Transport<PeerId = C::PeerId>,
{
    fn create(&self, scope: &StorageScope) -> Result<T, String>;

    fn synchronize(
        &self,
        scope: StorageScope,
        file_system: Arc<ScopedFileSystem<C, T>>,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>>;
}

pub struct ScopedFileSystemRuntime<C, T, F>
where
    C: PeerCodec,
    T: Transport<PeerId = C::PeerId>,
    F: ScopedTransportFactory<C, T>,
{
    root: PathBuf,
    keyring: RawKeyringRepo,
    local_peer: C::PeerId,
    transport_factory: F,
    file_systems: Mutex<BTreeMap<StorageScopeId, Arc<ScopedFileSystem<C, T>>>>,
}

impl<C, T, F> ScopedFileSystemRuntime<C, T, F>
where
    C: PeerCodec + Send + Sync + 'static,
    C::Error: Send + 'static,
    C::PeerId: Clone + Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Error: std::fmt::Display,
    T::Incoming: Send + 'static,
    F: ScopedTransportFactory<C, T>,
{
    pub fn new(
        root: PathBuf,
        keyring_service_name: impl Into<String>,
        local_peer: C::PeerId,
        transport_factory: F,
    ) -> io::Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            keyring: RawKeyringRepo::new(keyring_service_name),
            local_peer,
            transport_factory,
            file_systems: Mutex::new(BTreeMap::new()),
        })
    }

    pub async fn open(&self, scope: &StorageScope) -> Result<Arc<ScopedFileSystem<C, T>>, String> {
        let id = storage_scope_id(scope)?;
        let mut file_systems = self.file_systems.lock().await;
        if let Some(file_system) = file_systems.get(&id) {
            let file_system = Arc::clone(file_system);
            drop(file_systems);
            self.transport_factory
                .synchronize(scope.clone(), Arc::clone(&file_system))
                .await?;
            return Ok(file_system);
        }
        let key = self.load_or_create_key(&id)?;
        let storage = NativeStorage::new(
            self.root
                .join("vaults")
                .join(&id.user_sub)
                .join(id.application_id.to_string()),
        )
        .map_err(|error| error.to_string())?;
        let file_system = Arc::new(
            FileSystem::new(
                EncryptedStorage::new(storage, key),
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

    fn load_or_create_key(&self, id: &StorageScopeId) -> Result<[u8; 32], String> {
        let application_id = id.application_id.to_string();
        if let Some(key) = self
            .keyring
            .load(&id.user_sub, &application_id, VAULT_KEY_NAME)
            .map_err(|error| error.to_string())?
        {
            return key
                .try_into()
                .map_err(|_| "invalid vault key length".to_string());
        }
        let mut key = [0_u8; 32];
        getrandom::fill(&mut key).map_err(|error| error.to_string())?;
        self.keyring
            .store(&id.user_sub, &application_id, VAULT_KEY_NAME, &key)
            .map_err(|error| error.to_string())?;
        Ok(key)
    }
}

fn storage_scope_id(scope: &StorageScope) -> Result<StorageScopeId, String> {
    let user_sub = &scope.user_sub;
    let valid = |value: &str| {
        !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    };
    if !valid(user_sub) {
        return Err("invalid user subject".to_string());
    }
    if scope.application_id <= 0 {
        return Err("invalid application id".to_string());
    }

    Ok(StorageScopeId {
        user_sub: user_sub.to_owned(),
        application_id: scope.application_id,
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

impl ScopedTransportFactory<LocalPeerCodec, LocalTransport> for LocalTransportFactory {
    fn create(&self, _: &StorageScope) -> Result<LocalTransport, String> {
        Ok(LocalTransport::new())
    }

    fn synchronize(
        &self,
        _: StorageScope,
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
