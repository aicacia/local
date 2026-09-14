use std::future::Future;
use std::{fmt, pin::Pin, sync::Arc};

use file_system::{
    FileEntry, FileSystem, FileSystemError, PeerCodec, ReadError, Storage, Transport,
};
use storage_model::{
    StorageEntry, StorageNamespace, StorageRequest, StorageResponse, StorageSession,
};

use crate::{Residency, ScopedFileSystem, ScopedFileSystemRuntime, ScopedTransportFactory};

pub struct StorageService<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>> {
    file_system: Arc<FileSystem<S, C, T>>,
}

impl<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>> Clone for StorageService<S, C, T> {
    fn clone(&self) -> Self {
        Self {
            file_system: Arc::clone(&self.file_system),
        }
    }
}

impl<S, C, T> StorageService<S, C, T>
where
    S: Storage + Send + Sync + 'static,
    S::Error: Send + 'static,
    C: PeerCodec + Send + 'static,
    C::Error: Send + 'static,
    C::PeerId: Send + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: 'static,
{
    #[must_use]
    pub fn new(file_system: Arc<FileSystem<S, C, T>>) -> Self {
        Self { file_system }
    }

    pub async fn execute(
        &self,
        request: StorageRequest,
    ) -> Result<StorageResponse, StorageServiceError<S::Error>> {
        match request {
            StorageRequest::Read { path } => self
                .file_system
                .start_read(&path)
                .await
                .map_err(StorageServiceError::Read)?
                .await
                .map(|content| StorageResponse::Read { content })
                .map_err(StorageServiceError::Read),
            StorageRequest::Write { path, content } => self
                .file_system
                .write(&path, &content)
                .await
                .map(storage_entry)
                .map(|entry| StorageResponse::Written { entry })
                .map_err(StorageServiceError::FileSystem),
            StorageRequest::Append { path, content } => self
                .file_system
                .append(&path, &content)
                .await
                .map(storage_entry)
                .map(|entry| StorageResponse::Appended { entry })
                .map_err(StorageServiceError::FileSystem),
            StorageRequest::Delete { path } => self
                .file_system
                .delete(&path)
                .await
                .map(|()| StorageResponse::Deleted)
                .map_err(StorageServiceError::FileSystem),
            StorageRequest::Entry { path } => self
                .file_system
                .entry(&path)
                .await
                .map(storage_entry)
                .map(|entry| StorageResponse::Entry { entry })
                .map_err(StorageServiceError::FileSystem),
            StorageRequest::List { path } => self
                .file_system
                .list(&path)
                .await
                .map(|entries| entries.into_iter().map(storage_entry).collect())
                .map(|entries| StorageResponse::Listed { entries })
                .map_err(StorageServiceError::FileSystem),
        }
    }
}

impl<S, C, T> StorageSession for StorageService<S, C, T>
where
    S: Storage + Send + Sync + 'static,
    S::Error: Send + Sync + 'static,
    C: PeerCodec + Send + Sync + 'static,
    C::Error: Send + Sync + 'static,
    C::PeerId: Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: Send + 'static,
{
    fn execute_session(
        &self,
        request: StorageRequest,
    ) -> Pin<Box<dyn Future<Output = StorageResponse> + Send + '_>> {
        Box::pin(async move {
            self.execute(request)
                .await
                .unwrap_or(StorageResponse::Error {
                    code: storage_model::StorageErrorCode::OperationFailed,
                })
        })
    }
}

#[derive(Debug)]
pub enum StorageServiceError<E> {
    FileSystem(FileSystemError<E>),
    Read(file_system::ReadError<E>),
    Storage(E),
}

impl<E: fmt::Display> fmt::Display for StorageServiceError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileSystem(error) => error.fmt(formatter),
            Self::Read(error) => error.fmt(formatter),
            Self::Storage(error) => error.fmt(formatter),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for StorageServiceError<E> {}

pub struct ScopedStorageService<C, T, F, S>
where
    C: PeerCodec,
    T: Transport<PeerId = C::PeerId>,
    F: ScopedTransportFactory<C, T, S>,
{
    runtime: Arc<ScopedFileSystemRuntime<C, T, F, S>>,
    scope: S,
    file_system: Arc<ScopedFileSystem<C, T>>,
}

impl<C, T, F, S> ScopedStorageService<C, T, F, S>
where
    C: PeerCodec + Send + Sync + 'static,
    C::Error: Send + 'static,
    C::PeerId: Clone + Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Error: fmt::Display,
    T::Incoming: Send + 'static,
    F: ScopedTransportFactory<C, T, S>,
    S: StorageNamespace + Clone + Send + Sync + 'static,
{
    #[must_use]
    pub fn new(
        runtime: Arc<ScopedFileSystemRuntime<C, T, F, S>>,
        scope: S,
        file_system: Arc<ScopedFileSystem<C, T>>,
    ) -> Self {
        Self {
            runtime,
            scope,
            file_system,
        }
    }

    pub async fn execute(
        &self,
        request: StorageRequest,
    ) -> Result<StorageResponse, StorageServiceError<std::io::Error>> {
        match request {
            StorageRequest::Read { path } => self
                .file_system
                .start_read(&path)
                .await
                .map_err(StorageServiceError::Read)?
                .await
                .map(|content| StorageResponse::Read { content })
                .map_err(StorageServiceError::Read),
            StorageRequest::Write { path, content } => self
                .write(&path, &content)
                .await
                .map(storage_entry)
                .map(|entry| StorageResponse::Written { entry }),
            StorageRequest::Append { path, content } => self
                .append(&path, &content)
                .await
                .map(storage_entry)
                .map(|entry| StorageResponse::Appended { entry }),
            StorageRequest::Delete { path } => self
                .file_system
                .delete(&path)
                .await
                .map(|()| StorageResponse::Deleted)
                .map_err(StorageServiceError::FileSystem),
            StorageRequest::Entry { path } => self
                .file_system
                .entry(&path)
                .await
                .map(storage_entry)
                .map(|entry| StorageResponse::Entry { entry })
                .map_err(StorageServiceError::FileSystem),
            StorageRequest::List { path } => self
                .file_system
                .list(&path)
                .await
                .map(|entries| entries.into_iter().map(storage_entry).collect())
                .map(|entries| StorageResponse::Listed { entries })
                .map_err(StorageServiceError::FileSystem),
        }
    }

    async fn write(
        &self,
        path: &str,
        content: &[u8],
    ) -> Result<FileEntry<C::PeerId>, StorageServiceError<std::io::Error>> {
        match self.residency(path)? {
            Residency::Full => self.file_system.write(path, content).await,
            Residency::Passthrough => {
                self.file_system
                    .write_passthrough_to_peer_or_any(path, content)
                    .await
            }
        }
        .map_err(StorageServiceError::FileSystem)
    }

    async fn append(
        &self,
        path: &str,
        content: &[u8],
    ) -> Result<FileEntry<C::PeerId>, StorageServiceError<std::io::Error>> {
        match self.residency(path)? {
            Residency::Full => self.file_system.append(path, content).await,
            Residency::Passthrough => {
                let mut current = self.read_for_append(path).await?;
                current.extend_from_slice(content);
                self.file_system
                    .write_passthrough_to_peer_or_any(path, &current)
                    .await
            }
        }
        .map_err(StorageServiceError::FileSystem)
    }

    fn residency(&self, path: &str) -> Result<Residency, StorageServiceError<std::io::Error>> {
        self.runtime
            .residency(&self.scope, path)
            .map_err(|_| StorageServiceError::FileSystem(FileSystemError::InvalidPath))
    }

    async fn read_for_append(
        &self,
        path: &str,
    ) -> Result<Vec<u8>, StorageServiceError<std::io::Error>> {
        match self.file_system.start_read(path).await {
            Ok(read) => read.await.map_err(StorageServiceError::Read),
            Err(ReadError::NoProvider)
                if matches!(
                    self.file_system.entry(path).await,
                    Err(FileSystemError::NotFound)
                ) =>
            {
                Ok(Vec::new())
            }
            Err(error) => Err(StorageServiceError::Read(error)),
        }
    }
}

impl<C, T, F, S> StorageSession for ScopedStorageService<C, T, F, S>
where
    C: PeerCodec + Send + Sync + 'static,
    C::Error: Send + Sync + 'static,
    C::PeerId: Clone + Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Error: fmt::Display + Send + Sync + 'static,
    T::Incoming: Send + 'static,
    F: ScopedTransportFactory<C, T, S>,
    S: StorageNamespace + Clone + Send + Sync + 'static,
{
    fn execute_session(
        &self,
        request: StorageRequest,
    ) -> Pin<Box<dyn Future<Output = StorageResponse> + Send + '_>> {
        Box::pin(async move {
            self.execute(request)
                .await
                .unwrap_or(StorageResponse::Error {
                    code: storage_model::StorageErrorCode::OperationFailed,
                })
        })
    }
}

fn storage_entry<P>(entry: FileEntry<P>) -> StorageEntry {
    StorageEntry {
        name: entry.name,
        hash: entry.hash.to_string(),
        size: entry.size,
        local: entry.local,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        convert::Infallible,
        env, fs,
        future::Future,
        pin::Pin,
        sync::{Arc, Mutex},
        time::{SystemTime, UNIX_EPOCH},
    };

    use file_system::{FileSystem, InMemoryStorage, MemoryTransport, PeerCodec};
    use storage_model::{StorageNamespace, StorageRequest, StorageResponse};

    use crate::{
        Residency, ScopedFileSystem, ScopedFileSystemRuntime, ScopedStorageService,
        ScopedTransportFactory,
    };

    use super::StorageService;

    #[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
    struct Peer;

    impl PeerCodec for Peer {
        type Error = Infallible;
        type PeerId = Self;

        fn encode(_: &Self::PeerId) -> Vec<u8> {
            Vec::new()
        }

        fn decode(_: &[u8]) -> Result<Self::PeerId, Self::Error> {
            Ok(Self)
        }
    }

    #[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
    struct ScopedPeer(u8);

    impl PeerCodec for ScopedPeer {
        type Error = Infallible;
        type PeerId = Self;

        fn encode(peer: &Self::PeerId) -> Vec<u8> {
            vec![peer.0]
        }

        fn decode(encoded: &[u8]) -> Result<Self::PeerId, Self::Error> {
            Ok(Self(encoded.first().copied().unwrap_or_default()))
        }
    }

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
    struct MemoryTransportFactory(Arc<Mutex<Vec<MemoryTransport<ScopedPeer>>>>);

    impl ScopedTransportFactory<ScopedPeer, MemoryTransport<ScopedPeer>, Namespace>
        for MemoryTransportFactory
    {
        fn create(&self, _: &Namespace) -> Result<MemoryTransport<ScopedPeer>, String> {
            self.0
                .lock()
                .expect("memory transport lock poisoned")
                .pop()
                .ok_or_else(|| "memory transport is missing".to_owned())
        }

        fn synchronize(
            &self,
            _: Namespace,
            _: Arc<ScopedFileSystem<ScopedPeer, MemoryTransport<ScopedPeer>>>,
        ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
            Box::pin(async { Ok(()) })
        }
    }

    fn temporary_root(name: &str) -> std::path::PathBuf {
        env::temp_dir().join(format!(
            "storage-service-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[tokio::test]
    async fn dispatches_file_system_operations() {
        let (transport, _) = MemoryTransport::pair(Peer, Peer);
        let file_system: Arc<FileSystem<InMemoryStorage, Peer, MemoryTransport<Peer>>> = Arc::new(
            FileSystem::new(InMemoryStorage::new(), Peer, transport)
                .await
                .unwrap(),
        );
        let service = StorageService::new(file_system);

        let written = service
            .execute(StorageRequest::Write {
                path: "notes/today.txt".into(),
                content: b"hello".to_vec(),
            })
            .await
            .unwrap();
        assert!(matches!(written, StorageResponse::Written { .. }));

        let read = service
            .execute(StorageRequest::Read {
                path: "notes/today.txt".into(),
            })
            .await
            .unwrap();
        assert_eq!(
            read,
            StorageResponse::Read {
                content: b"hello".to_vec()
            }
        );
    }

    #[tokio::test]
    async fn scoped_service_durably_writes_passthrough_content() {
        let root = temporary_root("passthrough-write");
        let (receiver_transport, sender_transport) =
            MemoryTransport::pair(ScopedPeer(1), ScopedPeer(2));
        let receiver = Arc::new(
            ScopedFileSystemRuntime::new(
                root.join("receiver"),
                ScopedPeer(1),
                MemoryTransportFactory(Arc::new(Mutex::new(vec![receiver_transport]))),
            )
            .unwrap(),
        );
        let sender = Arc::new(
            ScopedFileSystemRuntime::new(
                root.join("sender"),
                ScopedPeer(2),
                MemoryTransportFactory(Arc::new(Mutex::new(vec![sender_transport]))),
            )
            .unwrap(),
        );
        let namespace = Namespace;
        receiver
            .set_residency(&namespace, "", Residency::Full)
            .await
            .unwrap();
        let receiver_file_system = receiver.open(&namespace).await.unwrap();
        let sender_file_system = sender.open(&namespace).await.unwrap();
        let service = ScopedStorageService::new(sender, namespace, sender_file_system);

        assert!(matches!(
            service
                .execute(StorageRequest::Write {
                    path: "notes/today.txt".into(),
                    content: b"hello".to_vec(),
                })
                .await,
            Ok(StorageResponse::Written { .. })
        ));
        for _ in 0..100 {
            if receiver_file_system.entry("notes/today.txt").await.is_ok() {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(
            receiver_file_system.read("notes/today.txt").await.unwrap(),
            b"hello"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn scoped_service_fails_passthrough_write_when_peer_rejects_it() {
        let root = temporary_root("passthrough-write-failure");
        let (receiver_transport, sender_transport) =
            MemoryTransport::pair(ScopedPeer(1), ScopedPeer(2));
        let receiver = Arc::new(
            ScopedFileSystemRuntime::new(
                root.join("receiver"),
                ScopedPeer(1),
                MemoryTransportFactory(Arc::new(Mutex::new(vec![receiver_transport]))),
            )
            .unwrap(),
        );
        let sender = Arc::new(
            ScopedFileSystemRuntime::new(
                root.join("sender"),
                ScopedPeer(2),
                MemoryTransportFactory(Arc::new(Mutex::new(vec![sender_transport]))),
            )
            .unwrap(),
        );
        let namespace = Namespace;
        let receiver_file_system = receiver.open(&namespace).await.unwrap();
        let sender_file_system = sender.open(&namespace).await.unwrap();
        let service = ScopedStorageService::new(sender, namespace, Arc::clone(&sender_file_system));

        assert!(
            service
                .execute(StorageRequest::Write {
                    path: "notes/today.txt".into(),
                    content: b"hello".to_vec(),
                })
                .await
                .is_err()
        );
        assert!(receiver_file_system.entry("notes/today.txt").await.is_err());
        assert!(sender_file_system.entry("notes/today.txt").await.is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn scoped_service_appends_passthrough_content_as_a_complete_upload() {
        let root = temporary_root("passthrough-append");
        let (receiver_transport, sender_transport) =
            MemoryTransport::pair(ScopedPeer(1), ScopedPeer(2));
        let receiver = Arc::new(
            ScopedFileSystemRuntime::new(
                root.join("receiver"),
                ScopedPeer(1),
                MemoryTransportFactory(Arc::new(Mutex::new(vec![receiver_transport]))),
            )
            .unwrap(),
        );
        let sender = Arc::new(
            ScopedFileSystemRuntime::new(
                root.join("sender"),
                ScopedPeer(2),
                MemoryTransportFactory(Arc::new(Mutex::new(vec![sender_transport]))),
            )
            .unwrap(),
        );
        let namespace = Namespace;
        receiver
            .set_residency(&namespace, "", Residency::Full)
            .await
            .unwrap();
        let receiver_file_system = receiver.open(&namespace).await.unwrap();
        let sender_file_system = sender.open(&namespace).await.unwrap();
        let service = ScopedStorageService::new(sender, namespace, sender_file_system);

        assert!(matches!(
            service
                .execute(StorageRequest::Append {
                    path: "notes/today.txt".into(),
                    content: b"hello".to_vec(),
                })
                .await,
            Ok(StorageResponse::Appended { .. })
        ));
        for _ in 0..100 {
            if receiver_file_system.entry("notes/today.txt").await.is_ok() {
                break;
            }
            tokio::task::yield_now().await;
        }
        let appended = service
            .execute(StorageRequest::Append {
                path: "notes/today.txt".into(),
                content: b" world".to_vec(),
            })
            .await;
        assert!(
            matches!(appended, Ok(StorageResponse::Appended { .. })),
            "{appended:?}"
        );
        for _ in 0..100 {
            if matches!(
                receiver_file_system.read("notes/today.txt").await,
                Ok(content) if content == b"hello world"
            ) {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(
            receiver_file_system.read("notes/today.txt").await.unwrap(),
            b"hello world"
        );
        let _ = fs::remove_dir_all(root);
    }
}
