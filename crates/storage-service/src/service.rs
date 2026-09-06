use std::{fmt, sync::Arc};

use file_system::{FileEntry, FileSystem, FileSystemError, PeerCodec, Storage, Transport};
use storage_model::{StorageEntry, StorageRequest, StorageResponse};

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
                .read(&path)
                .await
                .map(|content| StorageResponse::Read { content })
                .map_err(StorageServiceError::Storage),
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

#[derive(Debug)]
pub enum StorageServiceError<E> {
    FileSystem(FileSystemError<E>),
    Storage(E),
}

impl<E: fmt::Display> fmt::Display for StorageServiceError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileSystem(error) => error.fmt(formatter),
            Self::Storage(error) => error.fmt(formatter),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for StorageServiceError<E> {}

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
    use std::{convert::Infallible, sync::Arc};

    use file_system::{FileSystem, InMemoryStorage, MemoryTransport, PeerCodec};
    use storage_model::{StorageRequest, StorageResponse};

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
}
