use crate::storage::BlobStore;
use crate::{ChunkStream, ContentHash, Error, FileEntry, PeerId};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec::Vec,
};

#[derive(Debug)]
pub struct InMemoryFileSystem {
    blobs: BlobStore,
    folders: BTreeMap<String, BTreeMap<String, FileEntry>>,
    local_peer: PeerId,
}

impl InMemoryFileSystem {
    #[must_use]
    pub fn new(local_peer: PeerId) -> Self {
        Self {
            blobs: BlobStore::default(),
            folders: BTreeMap::new(),
            local_peer,
        }
    }

    pub fn write(&mut self, path: &str, content: &[u8]) -> Result<FileEntry, Error> {
        let (folder, name) = split_path(path)?;
        let hash = self.blobs.insert(content);
        let size = u64::try_from(content.len()).map_err(|_| Error::ContentTooLarge)?;
        let mut providers = BTreeSet::new();
        providers.insert(self.local_peer);
        let entry = FileEntry::new(name.to_string(), hash, size, providers, true);
        self.folders
            .entry(folder.to_string())
            .or_default()
            .insert(name.to_string(), entry.clone());
        Ok(entry)
    }

    pub fn register_passthrough(
        &mut self,
        path: &str,
        hash: ContentHash,
        size: u64,
        providers: BTreeSet<PeerId>,
    ) -> Result<FileEntry, Error> {
        let (folder, name) = split_path(path)?;
        let entry = FileEntry::new(name.to_string(), hash, size, providers, false);
        self.folders
            .entry(folder.to_string())
            .or_default()
            .insert(name.to_string(), entry.clone());
        Ok(entry)
    }

    pub fn entry(&self, path: &str) -> Result<&FileEntry, Error> {
        let (folder, name) = split_path(path)?;
        self.folders
            .get(folder)
            .and_then(|entries| entries.get(name))
            .ok_or(Error::NotFound)
    }

    pub fn list(&self, folder: &str) -> Result<Vec<FileEntry>, Error> {
        validate_folder(folder)?;
        Ok(self
            .folders
            .get(folder)
            .map(|entries| entries.values().cloned().collect())
            .unwrap_or_default())
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>, Error> {
        let entry = self.entry(path)?;
        if !entry.local {
            return Err(Error::ContentUnavailable);
        }
        let content = self
            .blobs
            .get(entry.hash)
            .ok_or(Error::ContentUnavailable)?;
        if ContentHash::of(content) != entry.hash {
            return Err(Error::CorruptContent);
        }
        Ok(content.to_vec())
    }

    pub fn stream(&self, path: &str, chunk_size: usize) -> Result<ChunkStream, Error> {
        if chunk_size == 0 {
            return Err(Error::InvalidChunkSize);
        }
        Ok(ChunkStream::new(self.read(path)?, chunk_size))
    }
}

fn split_path(path: &str) -> Result<(&str, &str), Error> {
    if path.is_empty() || path.starts_with('/') || path.ends_with('/') {
        return Err(Error::InvalidPath);
    }
    let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
    validate_folder(folder)?;
    if !is_name(name) {
        return Err(Error::InvalidPath);
    }
    Ok((folder, name))
}

fn validate_folder(folder: &str) -> Result<(), Error> {
    if folder.is_empty() || folder.split('/').all(is_name) {
        Ok(())
    } else {
        Err(Error::InvalidPath)
    }
}

fn is_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".."
}

#[cfg(test)]
mod tests {
    use alloc::{collections::BTreeSet, vec};
    use core::{
        pin::Pin,
        task::{Context, Poll, Waker},
    };

    use futures_core::Stream;

    use super::InMemoryFileSystem;
    use crate::{ContentHash, Error, MergeStrategy, PeerId};

    const PEER: PeerId = PeerId([7; 32]);

    #[test]
    fn writes_and_reads_content_addressed_files() {
        let mut file_system = InMemoryFileSystem::new(PEER);
        let entry = file_system.write("documents/note.txt", b"hello").unwrap();

        assert_eq!(entry.hash, ContentHash::of(b"hello"));
        assert_eq!(entry.size, 5);
        assert!(entry.local);
        assert_eq!(file_system.read("documents/note.txt").unwrap(), b"hello");
        assert_eq!(file_system.list("documents").unwrap(), vec![entry]);
    }

    #[test]
    fn passthrough_content_is_not_read_locally() {
        let mut file_system = InMemoryFileSystem::new(PEER);
        let mut providers = BTreeSet::new();
        providers.insert(PeerId([8; 32]));
        file_system
            .register_passthrough("remote.bin", ContentHash::of(b"remote"), 6, providers)
            .unwrap();

        assert_eq!(
            file_system.read("remote.bin"),
            Err(Error::ContentUnavailable)
        );
    }

    #[test]
    fn streams_content_in_order() {
        let mut file_system = InMemoryFileSystem::new(PEER);
        file_system.write("data", b"abcdef").unwrap();
        let mut stream = file_system.stream("data", 2).unwrap();
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);

        assert_eq!(
            Pin::new(&mut stream).poll_next(&mut context),
            Poll::Ready(Some(b"ab".to_vec()))
        );
        assert_eq!(
            Pin::new(&mut stream).poll_next(&mut context),
            Poll::Ready(Some(b"cd".to_vec()))
        );
        assert_eq!(
            Pin::new(&mut stream).poll_next(&mut context),
            Poll::Ready(Some(b"ef".to_vec()))
        );
        assert_eq!(
            Pin::new(&mut stream).poll_next(&mut context),
            Poll::Ready(None)
        );
    }

    #[test]
    fn derives_merge_strategy_from_extension() {
        let mut file_system = InMemoryFileSystem::new(PEER);
        let entry = file_system.write("state.automerge", b"document").unwrap();

        assert_eq!(entry.merge_strategy, MergeStrategy::AutomergeDocument);
    }

    #[test]
    fn rejects_unsafe_paths_and_zero_size_chunks() {
        let mut file_system = InMemoryFileSystem::new(PEER);

        assert_eq!(
            file_system.write("../secret", b"content"),
            Err(Error::InvalidPath)
        );
        file_system.write("safe", b"content").unwrap();
        assert!(matches!(
            file_system.stream("safe", 0),
            Err(Error::InvalidChunkSize)
        ));
    }
}
