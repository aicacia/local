use std::{fmt, future::Future, sync::Arc};

use file_system::{Entry, FileKind, FileSystem};
use serde::{Serialize, de::DeserializeOwned};
use storage_model::{
    StorageEntry, StorageErrorCode, StorageNamespace, StorageRequest, StorageResponse,
    StorageSession,
};

use crate::{ScopedFileSystem, ScopedFileSystemRuntime};

pub struct StorageService<PeerId>
where
    PeerId: Clone + fmt::Debug + Ord + Serialize + DeserializeOwned + 'static,
{
    file_system: Arc<FileSystem<PeerId>>,
}

impl<PeerId> Clone for StorageService<PeerId>
where
    PeerId: Clone + fmt::Debug + Ord + Serialize + DeserializeOwned + 'static,
{
    fn clone(&self) -> Self {
        Self {
            file_system: Arc::clone(&self.file_system),
        }
    }
}

impl<PeerId> StorageService<PeerId>
where
    PeerId: Clone + fmt::Debug + Ord + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    #[must_use]
    pub fn new(file_system: Arc<FileSystem<PeerId>>) -> Self {
        Self { file_system }
    }

    pub async fn execute(
        &self,
        request: StorageRequest,
    ) -> Result<StorageResponse, StorageServiceError> {
        execute(&self.file_system, request).await
    }

    pub async fn stream(
        &self,
        path: &str,
        chunk_size: usize,
    ) -> Result<file_system::ReadStream, StorageServiceError> {
        self.file_system
            .stream(path, chunk_size)
            .await
            .map_err(StorageServiceError::FileSystem)
    }
}

impl<PeerId> StorageSession for StorageService<PeerId>
where
    PeerId: Clone + fmt::Debug + Ord + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    fn execute_session(
        &self,
        request: StorageRequest,
    ) -> impl Future<Output = StorageResponse> + Send {
        async move {
            self.execute(request)
                .await
                .unwrap_or_else(StorageServiceError::response)
        }
    }
}

pub struct ScopedStorageService<PeerId, S>
where
    PeerId: Clone + fmt::Debug + Ord + Serialize + DeserializeOwned + 'static,
{
    _scope: S,
    file_system: Arc<ScopedFileSystem<PeerId>>,
}

impl<PeerId, S> ScopedStorageService<PeerId, S>
where
    PeerId: Clone + fmt::Debug + Ord + Serialize + DeserializeOwned + Send + Sync + 'static,
    S: StorageNamespace + Send + Sync + 'static,
{
    pub async fn open(
        runtime: &ScopedFileSystemRuntime<PeerId>,
        scope: S,
    ) -> Result<Self, StorageServiceError> {
        let file_system = runtime
            .open(&scope)
            .await
            .map_err(StorageServiceError::Scope)?;
        Ok(Self {
            _scope: scope,
            file_system,
        })
    }

    pub async fn execute(
        &self,
        request: StorageRequest,
    ) -> Result<StorageResponse, StorageServiceError> {
        execute(&self.file_system, request).await
    }

    pub async fn stream(
        &self,
        path: &str,
        chunk_size: usize,
    ) -> Result<file_system::ReadStream, StorageServiceError> {
        self.file_system
            .stream(path, chunk_size)
            .await
            .map_err(StorageServiceError::FileSystem)
    }
}

impl<PeerId, S> StorageSession for ScopedStorageService<PeerId, S>
where
    PeerId: Clone + fmt::Debug + Ord + Serialize + DeserializeOwned + Send + Sync + 'static,
    S: StorageNamespace + Send + Sync + 'static,
{
    fn execute_session(
        &self,
        request: StorageRequest,
    ) -> impl Future<Output = StorageResponse> + Send {
        async move {
            self.execute(request)
                .await
                .unwrap_or_else(StorageServiceError::response)
        }
    }
}

async fn execute<PeerId>(
    file_system: &FileSystem<PeerId>,
    request: StorageRequest,
) -> Result<StorageResponse, StorageServiceError>
where
    PeerId: Clone + fmt::Debug + Ord + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    match request {
        StorageRequest::Read { path } => Ok(StorageResponse::Read {
            content: file_system
                .read(&path)
                .await
                .map_err(StorageServiceError::FileSystem)?,
        }),
        StorageRequest::Write { path, content } => Ok(StorageResponse::Written {
            entry: storage_entry(
                file_system,
                file_system
                    .write(&path, &content)
                    .await
                    .map_err(StorageServiceError::FileSystem)?,
            )
            .await?,
        }),
        StorageRequest::Append { path, content } => {
            file_system
                .append(&path, &content)
                .await
                .map_err(StorageServiceError::FileSystem)?;
            Ok(StorageResponse::Appended {
                entry: storage_entry(
                    file_system,
                    file_system
                        .entry(&path)
                        .await
                        .map_err(StorageServiceError::FileSystem)?,
                )
                .await?,
            })
        }
        StorageRequest::Delete { path } => {
            file_system
                .delete(&path)
                .await
                .map_err(StorageServiceError::FileSystem)?;
            Ok(StorageResponse::Deleted)
        }
        StorageRequest::CreateDir { path } => Ok(StorageResponse::DirectoryCreated {
            entry: storage_entry(
                file_system,
                file_system
                    .create_dir(&path)
                    .await
                    .map_err(StorageServiceError::FileSystem)?,
            )
            .await?,
        }),
        StorageRequest::Rename { from, to } => {
            file_system
                .rename(&from, &to)
                .await
                .map_err(StorageServiceError::FileSystem)?;
            Ok(StorageResponse::Renamed)
        }
        StorageRequest::Entry { path } => Ok(StorageResponse::Entry {
            entry: storage_entry(
                file_system,
                file_system
                    .entry(&path)
                    .await
                    .map_err(StorageServiceError::FileSystem)?,
            )
            .await?,
        }),
        StorageRequest::List { path } => {
            let mut entries = Vec::new();
            for entry in file_system
                .list(&path)
                .await
                .map_err(StorageServiceError::FileSystem)?
            {
                entries.push(storage_entry(file_system, entry).await?);
            }
            Ok(StorageResponse::Listed { entries })
        }
    }
}

async fn storage_entry<PeerId>(
    file_system: &FileSystem<PeerId>,
    entry: Entry<PeerId>,
) -> Result<StorageEntry, StorageServiceError>
where
    PeerId: Clone + fmt::Debug + Ord + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    let size = if entry.meta.kind == FileKind::File {
        file_system
            .read(&entry.path)
            .await
            .map_err(StorageServiceError::FileSystem)?
            .len() as u64
    } else {
        0
    };
    Ok(StorageEntry {
        name: entry.path,
        hash: entry.meta.pointer.unwrap_or_default(),
        size,
        local: entry.meta.local,
    })
}

#[derive(Debug)]
pub enum StorageServiceError {
    FileSystem(file_system::Error),
    Scope(String),
}

impl StorageServiceError {
    fn response(self) -> StorageResponse {
        StorageResponse::Error {
            code: StorageErrorCode::OperationFailed,
        }
    }
}

impl fmt::Display for StorageServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileSystem(error) => error.fmt(formatter),
            Self::Scope(error) => formatter.write_str(error),
        }
    }
}

impl std::error::Error for StorageServiceError {}

#[cfg(test)]
mod tests {
    use std::{env, fs, sync::Arc};

    use file_system::{FileSystem, Residency};
    use storage_model::{StorageRequest, StorageResponse};

    use super::StorageService;

    #[tokio::test]
    async fn dispatches_scoped_file_system_operations() {
        let root = env::temp_dir().join(format!("storage-service-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let file_system = Arc::new(FileSystem::open(&root, 1_u8).unwrap());
        file_system
            .set_residency("", Residency::Full)
            .await
            .unwrap();
        let service = StorageService::new(Arc::clone(&file_system));

        assert!(matches!(
            service
                .execute(StorageRequest::Write {
                    path: "notes/today.txt".into(),
                    content: b"hello".to_vec()
                })
                .await,
            Ok(StorageResponse::Written { .. })
        ));
        assert_eq!(
            service
                .execute(StorageRequest::Read {
                    path: "notes/today.txt".into()
                })
                .await
                .unwrap(),
            StorageResponse::Read {
                content: b"hello".to_vec()
            }
        );
        assert!(matches!(
            service
                .execute(StorageRequest::List {
                    path: "notes".into()
                })
                .await,
            Ok(StorageResponse::Listed { .. })
        ));
        assert!(service.stream("notes/today.txt", 1).await.is_ok());
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn passthrough_rejects_writes() {
        let root = env::temp_dir().join(format!(
            "storage-service-passthrough-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let service = StorageService::new(Arc::new(FileSystem::open(&root, 1_u8).unwrap()));
        assert!(
            service
                .execute(StorageRequest::Write {
                    path: "note.txt".into(),
                    content: Vec::new()
                })
                .await
                .is_err()
        );
        let _ = fs::remove_dir_all(root);
    }
}
