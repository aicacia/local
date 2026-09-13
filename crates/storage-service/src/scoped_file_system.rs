use std::{
    collections::BTreeMap,
    convert::Infallible,
    future::Future,
    io,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex as StdMutex},
    task::{Context, Poll},
};

use file_system::{FileSystem, NativeStorage, PeerCodec, Transport};
use futures_core::Stream;
use storage_model::StorageNamespace;
use tokio::sync::{Mutex, mpsc};

use crate::residency::{Residency, ResidencyPolicy, validate_user_sub};

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
    residency_policy: Arc<StdMutex<ResidencyPolicy>>,
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
        let residency_policy = ResidencyPolicy::load(&root)?;
        Ok(Self {
            root,
            local_peer,
            transport_factory,
            file_systems: Mutex::new(BTreeMap::new()),
            residency_policy: Arc::new(StdMutex::new(residency_policy)),
            _scope: core::marker::PhantomData,
        })
    }

    pub fn residency(&self, scope: &S, path: &str) -> Result<Residency, String> {
        let id = storage_namespace_id(scope)?;
        self.residency_policy
            .lock()
            .expect("residency policy lock poisoned")
            .residency(&id.user_sub, id.application_id, path)
    }

    pub async fn set_residency(
        &self,
        scope: &S,
        path: &str,
        residency: Residency,
    ) -> Result<(), String> {
        let id = storage_namespace_id(scope)?;
        let next_policy = {
            let policy = self
                .residency_policy
                .lock()
                .expect("residency policy lock poisoned");
            let mut next = policy.clone();
            next.set_residency(&id.user_sub, id.application_id, path, residency)?;
            next
        };
        let file_system = self.file_systems.lock().await.get(&id).cloned();
        if let Some(file_system) = file_system {
            for file_path in file_system
                .paths()
                .await
                .map_err(|error| error.to_string())?
            {
                if !matches_residency_path(path, &file_path) {
                    continue;
                }
                let current = self.residency(scope, &file_path)?;
                let next = next_policy.residency(&id.user_sub, id.application_id, &file_path)?;
                match (current, next) {
                    (Residency::Passthrough, Residency::Full) => file_system
                        .materialize(&file_path)
                        .await
                        .map_err(|error| error.to_string())?,
                    (Residency::Full, Residency::Passthrough) => file_system
                        .evict(&file_path)
                        .await
                        .map_err(|error| error.to_string())?,
                    _ => {}
                }
            }
        }
        next_policy
            .save(&self.root)
            .map_err(|error| error.to_string())?;
        *self
            .residency_policy
            .lock()
            .expect("residency policy lock poisoned") = next_policy;
        Ok(())
    }

    pub fn set_application_excluded(
        &self,
        application_id: i64,
        excluded: bool,
    ) -> Result<(), String> {
        let mut policy = self
            .residency_policy
            .lock()
            .expect("residency policy lock poisoned");
        policy.set_application_excluded(application_id, excluded)?;
        policy.save(&self.root).map_err(|error| error.to_string())
    }

    pub async fn open(&self, scope: &S) -> Result<Arc<ScopedFileSystem<C, T>>, String> {
        let id = storage_namespace_id(scope)?;
        if self
            .residency_policy
            .lock()
            .expect("residency policy lock poisoned")
            .is_application_excluded(id.application_id)
        {
            return Err("application storage is excluded on this device".to_owned());
        }
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
        let policy = Arc::clone(&self.residency_policy);
        let user_sub = id.user_sub.clone();
        let application_id = id.application_id;
        file_system
            .set_passthrough_admission(move |path| {
                matches!(
                    policy
                        .lock()
                        .expect("residency policy lock poisoned")
                        .residency(&user_sub, application_id, path),
                    Ok(Residency::Full)
                )
            })
            .await;
        file_systems.insert(id, Arc::clone(&file_system));
        drop(file_systems);
        self.transport_factory
            .synchronize(scope.clone(), Arc::clone(&file_system))
            .await?;
        Ok(file_system)
    }
}

fn matches_residency_path(rule: &str, path: &str) -> bool {
    rule.is_empty() || path == rule || path.starts_with(&format!("{rule}/"))
}

fn storage_namespace_id(scope: &impl StorageNamespace) -> Result<StorageNamespaceId, String> {
    let user_sub = scope.user_sub();
    validate_user_sub(user_sub)?;
    if scope.application_id() <= 0 {
        return Err("invalid application id".to_owned());
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

#[cfg(test)]
mod tests {
    use std::{
        env, fs,
        future::Future,
        pin::Pin,
        sync::{Arc, Mutex},
    };

    use file_system::MemoryTransport;

    use super::{
        LocalPeer, LocalPeerCodec, LocalScopedFileSystemRuntime, LocalTransportFactory, Residency,
        ScopedFileSystem, ScopedFileSystemRuntime, ScopedTransportFactory, StorageNamespace,
    };

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

    #[derive(Clone)]
    struct MemoryTransportFactory(Arc<Mutex<Vec<MemoryTransport<LocalPeer>>>>);

    impl ScopedTransportFactory<LocalPeerCodec, MemoryTransport<LocalPeer>, Namespace>
        for MemoryTransportFactory
    {
        fn create(&self, _: &Namespace) -> Result<MemoryTransport<LocalPeer>, String> {
            self.0
                .lock()
                .expect("memory transport lock poisoned")
                .pop()
                .ok_or_else(|| "memory transport is missing".to_owned())
        }

        fn synchronize(
            &self,
            _: Namespace,
            _: Arc<ScopedFileSystem<LocalPeerCodec, MemoryTransport<LocalPeer>>>,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
            Box::pin(async { Ok(()) })
        }
    }

    #[tokio::test]
    async fn admits_passthrough_uploads_after_residency_becomes_full() {
        let root = env::temp_dir().join(format!(
            "scoped-file-system-admission-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let (left_transport, right_transport) = MemoryTransport::pair(LocalPeer, LocalPeer);
        let left = ScopedFileSystemRuntime::new(
            root.join("left"),
            LocalPeer,
            MemoryTransportFactory(Arc::new(Mutex::new(vec![left_transport]))),
        )
        .unwrap();
        let right = ScopedFileSystemRuntime::new(
            root.join("right"),
            LocalPeer,
            MemoryTransportFactory(Arc::new(Mutex::new(vec![right_transport]))),
        )
        .unwrap();
        let namespace = Namespace;
        let receiver = left.open(&namespace).await.unwrap();
        let sender = right.open(&namespace).await.unwrap();
        assert!(
            sender
                .write_passthrough_to_peer_or_any("notes/today.txt", b"hello")
                .await
                .is_err()
        );
        assert!(receiver.entry("notes/today.txt").await.is_err());

        left.set_residency(&namespace, "notes", Residency::Full)
            .await
            .unwrap();
        sender
            .write_passthrough_to_peer_or_any("notes/today.txt", b"hello")
            .await
            .unwrap();
        for _ in 0..100 {
            if receiver.entry("notes/today.txt").await.is_ok() {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(receiver.entry("notes/today.txt").await.unwrap().local);
        assert_eq!(receiver.read("notes/today.txt").await.unwrap(), b"hello");
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn excludes_applications_before_creating_a_vault() {
        let root = env::temp_dir().join(format!(
            "scoped-file-system-excluded-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let runtime =
            LocalScopedFileSystemRuntime::new(root.clone(), LocalPeer, LocalTransportFactory)
                .unwrap();
        runtime.set_application_excluded(1, true).unwrap();
        assert!(runtime.open(&Namespace).await.is_err());
        assert!(!root.join("vaults/user/1").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn persists_scoped_native_storage() {
        let root = env::temp_dir().join(format!(
            "scoped-file-system-persisted-{}",
            std::process::id()
        ));
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
