use std::sync::Arc;

use crate::repo::{RepoError, RepoResult};
use file_system::{FileSystem, PeerCodec, Storage, Transport};
use serde::{Serialize, de::DeserializeOwned};

pub struct JsonStore<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>> {
    file_system: Arc<FileSystem<S, C, T>>,
}

impl<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>> Clone for JsonStore<S, C, T> {
    fn clone(&self) -> Self {
        Self {
            file_system: Arc::clone(&self.file_system),
        }
    }
}

impl<S, C, T> JsonStore<S, C, T>
where
    S: Storage + Send + Sync + 'static,
    S::Error: std::fmt::Display + Send + 'static,
    C: PeerCodec + Send + 'static,
    C::Error: Send + 'static,
    C::PeerId: Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: 'static,
{
    #[must_use]
    pub fn new(file_system: Arc<FileSystem<S, C, T>>) -> Self {
        Self { file_system }
    }

    pub async fn read<V: DeserializeOwned>(&self, path: &str) -> RepoResult<V> {
        let bytes = self
            .file_system
            .read(path)
            .await
            .map_err(|error| RepoError::InvalidInput(error.to_string()))?;
        serde_json::from_slice(&bytes).map_err(RepoError::other)
    }

    pub async fn write<V: Serialize>(&self, path: &str, value: &V) -> RepoResult<()> {
        let bytes = serde_json::to_vec(value).map_err(RepoError::other)?;
        self.file_system
            .write(path, &bytes)
            .await
            .map_err(|error| RepoError::InvalidInput(error.to_string()))?;
        Ok(())
    }

    pub async fn delete(&self, path: &str) -> RepoResult<()> {
        self.file_system
            .delete(path)
            .await
            .map_err(|error| RepoError::InvalidInput(error.to_string()))
    }

    pub async fn list(&self, folder: &str) -> RepoResult<Vec<String>> {
        self.file_system
            .list(folder)
            .await
            .map(|entries| {
                entries
                    .into_iter()
                    .filter(|entry| !entry.tombstoned)
                    .map(|entry| format!("{folder}/{}", entry.name))
                    .collect()
            })
            .map_err(|error| RepoError::InvalidInput(error.to_string()))
    }
}
