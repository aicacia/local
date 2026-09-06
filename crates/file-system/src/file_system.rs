use alloc::{collections::BTreeSet, vec::Vec};

use crate::{ChunkStream, ContentHash, FileEntry, Storage, Transport};

#[must_use]
pub struct FileSystem<S, T> {
    storage: S,
    transport: T,
}

impl<S, T> FileSystem<S, T>
where
    S: Storage,
    T: Transport<PeerId = S::PeerId>,
{
    pub fn new(storage: S, transport: T) -> Self {
        Self { storage, transport }
    }

    pub fn storage(&self) -> &S {
        &self.storage
    }

    pub fn storage_mut(&mut self) -> &mut S {
        &mut self.storage
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    pub fn into_parts(self) -> (S, T) {
        (self.storage, self.transport)
    }

    pub fn write(&mut self, path: &str, content: &[u8]) -> Result<FileEntry<S::PeerId>, S::Error> {
        self.storage.write(path, content)
    }

    pub fn register_passthrough(
        &mut self,
        path: &str,
        hash: ContentHash,
        size: u64,
        providers: BTreeSet<S::PeerId>,
    ) -> Result<FileEntry<S::PeerId>, S::Error> {
        self.storage
            .register_passthrough(path, hash, size, providers)
    }

    pub fn entry(&self, path: &str) -> Result<FileEntry<S::PeerId>, S::Error> {
        self.storage.entry(path)
    }

    pub fn list(&self, folder: &str) -> Result<Vec<FileEntry<S::PeerId>>, S::Error> {
        self.storage.list(folder)
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, S::Error> {
        self.storage.read(path)
    }

    pub fn stream(&self, path: &str, chunk_size: usize) -> Result<ChunkStream, S::Error> {
        self.storage.stream(path, chunk_size)
    }
}
