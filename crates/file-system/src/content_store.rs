use alloc::{collections::BTreeSet, format, string::String, vec::Vec};

use crate::{ContentHash, Storage};

const BLOBS_DIRECTORY: &str = ".blobs";
const PATHS_DIRECTORY: &str = ".paths";
const PASSTHROUGH_DIRECTORY: &str = ".passthrough";

pub(crate) struct ContentStore<S> {
    storage: S,
}

impl<S> ContentStore<S> {
    pub(crate) fn new(storage: S) -> Self {
        Self { storage }
    }

    pub(crate) fn storage(&self) -> &S {
        &self.storage
    }

    pub(crate) fn storage_mut(&mut self) -> &mut S {
        &mut self.storage
    }
}

impl<S: Storage> ContentStore<S> {
    pub(crate) async fn write(
        &mut self,
        path: &str,
        content: &[u8],
    ) -> Result<ContentHash, S::Error> {
        let hash = ContentHash::of(content);
        self.store_blob(hash, content).await?;
        self.storage
            .write(&path_path(path), hash.as_bytes())
            .await?;
        let _ = self.storage.remove(&passthrough_path(path)).await;
        Ok(hash)
    }

    pub(crate) async fn store_blob(
        &mut self,
        hash: ContentHash,
        content: &[u8],
    ) -> Result<(), S::Error> {
        self.storage.write(&blob_path(hash), content).await
    }

    pub(crate) async fn link(&mut self, path: &str, hash: ContentHash) -> Result<(), S::Error> {
        self.storage
            .write(&path_path(path), hash.as_bytes())
            .await?;
        let _ = self.storage.remove(&passthrough_path(path)).await;
        Ok(())
    }

    pub(crate) async fn append(
        &mut self,
        path: &str,
        content: &[u8],
    ) -> Result<ContentHash, S::Error> {
        let mut existing = self.read(path).await?;
        existing.extend_from_slice(content);
        self.write(path, &existing).await
    }

    pub(crate) async fn delete(&mut self, path: &str) -> Result<(), S::Error> {
        let _ = self.storage.remove(&path_path(path)).await;
        let _ = self.storage.remove(&passthrough_path(path)).await;
        Ok(())
    }

    pub(crate) async fn rename(
        &mut self,
        from: &str,
        to: &str,
        local: bool,
    ) -> Result<(), S::Error> {
        if local {
            let content = self.storage.read(&path_path(from)).await?;
            self.storage.write(&path_path(to), &content).await?;
            self.storage.remove(&path_path(from)).await?;
        } else {
            self.storage.write(&passthrough_path(to), &[]).await?;
            self.storage.remove(&passthrough_path(from)).await?;
        }
        Ok(())
    }

    pub(crate) async fn read(&self, path: &str) -> Result<Vec<u8>, S::Error> {
        let hash = self.storage.read(&path_path(path)).await?;
        let hash: [u8; blake3::OUT_LEN] = hash.try_into().expect("invalid stored content hash");
        let hash = ContentHash::from_bytes(hash);
        let content = self.storage.read(&blob_path(hash)).await?;
        debug_assert_eq!(ContentHash::of(&content), hash);
        Ok(content)
    }

    pub(crate) async fn has_blob(&self, hash: &ContentHash) -> bool {
        self.storage.read(&blob_path(*hash)).await.is_ok()
    }

    pub(crate) async fn register_passthrough(&mut self, path: &str) -> Result<(), S::Error> {
        self.storage.write(&passthrough_path(path), &[]).await?;
        let _ = self.storage.remove(&path_path(path)).await;
        Ok(())
    }

    pub(crate) async fn remove_unreferenced(
        &mut self,
        referenced_hashes: &BTreeSet<ContentHash>,
    ) -> Result<(), S::Error> {
        let referenced_paths = referenced_hashes
            .iter()
            .map(|hash| blob_path(*hash))
            .collect::<BTreeSet<_>>();
        for path in self.storage.list(BLOBS_DIRECTORY).await? {
            if !referenced_paths.contains(&path) {
                self.storage.remove(&path).await?;
            }
        }
        Ok(())
    }
}

fn blob_path(hash: ContentHash) -> String {
    format!("{BLOBS_DIRECTORY}/{hash}")
}

fn path_path(path: &str) -> String {
    format!("{PATHS_DIRECTORY}/{path}")
}

fn passthrough_path(path: &str) -> String {
    format!("{PASSTHROUGH_DIRECTORY}/{path}")
}
