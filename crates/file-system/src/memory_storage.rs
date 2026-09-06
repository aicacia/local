use crate::{ChunkStream, ContentHash, Error, FileEntry, Storage};
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec::Vec,
};

#[derive(Debug, Default)]
struct BlobStore {
    blobs: BTreeMap<ContentHash, Vec<u8>>,
}

impl BlobStore {
    fn insert(&mut self, content: &[u8]) -> ContentHash {
        let hash = ContentHash::of(content);
        self.blobs.entry(hash).or_insert_with(|| content.to_vec());
        hash
    }

    fn get(&self, hash: ContentHash) -> Option<&[u8]> {
        self.blobs.get(&hash).map(Vec::as_slice)
    }
}

#[derive(Debug)]
pub struct InMemoryStorage<P> {
    blobs: BlobStore,
    folders: BTreeMap<String, BTreeMap<String, FileEntry<P>>>,
    local_peer: P,
}

impl<P: Ord + Clone> InMemoryStorage<P> {
    #[must_use]
    pub fn new(local_peer: P) -> Self {
        Self {
            blobs: BlobStore::default(),
            folders: BTreeMap::new(),
            local_peer,
        }
    }

    pub fn write(&mut self, path: &str, content: &[u8]) -> Result<FileEntry<P>, Error> {
        let (folder, name) = split_path(path)?;
        let hash = self.blobs.insert(content);
        let size = u64::try_from(content.len()).map_err(|_| Error::ContentTooLarge)?;
        let mut providers = BTreeSet::new();
        providers.insert(self.local_peer.clone());
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
        providers: BTreeSet<P>,
    ) -> Result<FileEntry<P>, Error> {
        let (folder, name) = split_path(path)?;
        let entry = FileEntry::new(name.to_string(), hash, size, providers, false);
        self.folders
            .entry(folder.to_string())
            .or_default()
            .insert(name.to_string(), entry.clone());
        Ok(entry)
    }

    pub fn entry(&self, path: &str) -> Result<&FileEntry<P>, Error> {
        let (folder, name) = split_path(path)?;
        self.folders
            .get(folder)
            .and_then(|entries| entries.get(name))
            .ok_or(Error::NotFound)
    }

    pub fn list(&self, folder: &str) -> Result<Vec<FileEntry<P>>, Error> {
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

impl<P: Ord + Clone> Storage for InMemoryStorage<P> {
    type Error = Error;
    type PeerId = P;

    fn write(&mut self, path: &str, content: &[u8]) -> Result<FileEntry<P>, Self::Error> {
        Self::write(self, path, content)
    }

    fn register_passthrough(
        &mut self,
        path: &str,
        hash: ContentHash,
        size: u64,
        providers: BTreeSet<P>,
    ) -> Result<FileEntry<P>, Self::Error> {
        Self::register_passthrough(self, path, hash, size, providers)
    }

    fn entry(&self, path: &str) -> Result<FileEntry<P>, Self::Error> {
        Ok(Self::entry(self, path)?.clone())
    }

    fn list(&self, folder: &str) -> Result<Vec<FileEntry<P>>, Self::Error> {
        Self::list(self, folder)
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, Self::Error> {
        Self::read(self, path)
    }

    fn stream(&self, path: &str, chunk_size: usize) -> Result<ChunkStream, Self::Error> {
        Self::stream(self, path, chunk_size)
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

    use super::InMemoryStorage;
    use crate::{ContentHash, Error, MergeStrategy};

    const PEER: u8 = 7;

    #[test]
    fn writes_and_reads_content_addressed_files() {
        let mut file_system = InMemoryStorage::new(PEER);
        let entry = file_system.write("documents/note.txt", b"hello").unwrap();

        assert_eq!(entry.hash, ContentHash::of(b"hello"));
        assert_eq!(entry.size, 5);
        assert!(entry.local);
        assert_eq!(file_system.read("documents/note.txt").unwrap(), b"hello");
        assert_eq!(file_system.list("documents").unwrap(), vec![entry]);
    }

    #[test]
    fn passthrough_content_is_not_read_locally() {
        let mut file_system = InMemoryStorage::new(PEER);
        let mut providers = BTreeSet::new();
        providers.insert(8);
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
        let mut file_system = InMemoryStorage::new(PEER);
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
        let mut file_system = InMemoryStorage::new(PEER);
        let entry = file_system.write("state.automerge", b"document").unwrap();

        assert_eq!(entry.merge_strategy, MergeStrategy::AutomergeDocument);
    }

    #[test]
    fn rejects_unsafe_paths_and_zero_size_chunks() {
        let mut file_system = InMemoryStorage::new(PEER);

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
