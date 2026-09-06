use alloc::{string::String, vec::Vec};

use crate::{ContentHash, Storage};

const BLOBS_DIRECTORY: &str = ".lidp/blobs";
const PATHS_DIRECTORY: &str = ".lidp/paths";
const PASSTHROUGH_DIRECTORY: &str = ".lidp/passthrough";

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
    pub(crate) fn write(&mut self, path: &str, content: &[u8]) -> Result<ContentHash, S::Error> {
        let hash = ContentHash::of(content);
        self.storage.write(&blob_path(hash), content)?;
        self.storage.write(&path_path(path), hash.as_bytes())?;
        let _ = self.storage.remove(&passthrough_path(path));
        Ok(hash)
    }

    pub(crate) fn append(&mut self, path: &str, content: &[u8]) -> Result<ContentHash, S::Error> {
        let mut existing = self.read(path)?;
        existing.extend_from_slice(content);
        self.write(path, &existing)
    }

    pub(crate) fn read(&self, path: &str) -> Result<Vec<u8>, S::Error> {
        let hash = self.storage.read(&path_path(path))?;
        let hash: [u8; blake3::OUT_LEN] = hash.try_into().expect("invalid stored content hash");
        let hash = ContentHash::from_bytes(hash);
        let content = self.storage.read(&blob_path(hash))?;
        debug_assert_eq!(ContentHash::of(&content), hash);
        Ok(content)
    }

    pub(crate) fn register_passthrough(&mut self, path: &str) -> Result<(), S::Error> {
        self.storage.write(&passthrough_path(path), &[])?;
        let _ = self.storage.remove(&path_path(path));
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
