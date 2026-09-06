use crate::{ChunkStream, ContentHash, Error, Storage};
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

#[derive(Debug, Default)]
pub struct InMemoryStorage {
    blobs: BlobStore,
    paths: BTreeMap<String, ContentHash>,
    passthrough_paths: BTreeSet<String>,
}

impl InMemoryStorage {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn write(&mut self, path: &str, content: &[u8]) -> Result<ContentHash, Error> {
        validate_path(path)?;
        let hash = self.blobs.insert(content);
        self.paths.insert(path.to_string(), hash);
        self.passthrough_paths.remove(path);
        Ok(hash)
    }

    pub fn append(&mut self, path: &str, content: &[u8]) -> Result<ContentHash, Error> {
        let mut existing = self.read_file(path)?;
        existing.extend_from_slice(content);
        self.write(path, &existing)
    }

    pub fn register_passthrough(&mut self, path: &str) -> Result<(), Error> {
        validate_path(path)?;
        self.paths.remove(path);
        self.passthrough_paths.insert(path.to_string());
        Ok(())
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, Error> {
        validate_path(path)?;
        let hash = match self.paths.get(path) {
            Some(hash) => *hash,
            None if self.passthrough_paths.contains(path) => return Err(Error::ContentUnavailable),
            None => return Err(Error::NotFound),
        };
        let content = self.blobs.get(hash).ok_or(Error::ContentUnavailable)?;
        if ContentHash::of(content) != hash {
            return Err(Error::CorruptContent);
        }
        Ok(content.to_vec())
    }

    pub fn stream(&self, path: &str, chunk_size: usize) -> Result<ChunkStream, Error> {
        if chunk_size == 0 {
            return Err(Error::InvalidChunkSize);
        }
        Ok(ChunkStream::new(self.read_file(path)?, chunk_size))
    }
}

impl Storage for InMemoryStorage {
    type Error = Error;

    fn write(&mut self, path: &str, content: &[u8]) -> Result<ContentHash, Self::Error> {
        Self::write(self, path, content)
    }

    fn append(&mut self, path: &str, content: &[u8]) -> Result<ContentHash, Self::Error> {
        Self::append(self, path, content)
    }

    fn register_passthrough(&mut self, path: &str) -> Result<(), Self::Error> {
        Self::register_passthrough(self, path)
    }

    fn read_file(&self, path: &str) -> Result<Vec<u8>, Self::Error> {
        Self::read_file(self, path)
    }

    fn stream(&self, path: &str, chunk_size: usize) -> Result<ChunkStream, Self::Error> {
        Self::stream(self, path, chunk_size)
    }
}

fn validate_path(path: &str) -> Result<(), Error> {
    if path.is_empty() || path.starts_with('/') || path.ends_with('/') {
        return Err(Error::InvalidPath);
    }
    if path.split('/').all(is_name) {
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

    use core::{
        pin::Pin,
        task::{Context, Poll, Waker},
    };

    use futures_core::Stream;

    use super::InMemoryStorage;
    use crate::{ContentHash, Error};

    #[test]
    fn writes_and_reads_content_addressed_files() {
        let mut storage = InMemoryStorage::new();
        let hash = storage.write("documents/note.txt", b"hello").unwrap();

        assert_eq!(hash, ContentHash::of(b"hello"));
        assert_eq!(storage.read_file("documents/note.txt").unwrap(), b"hello");
    }

    #[test]
    fn appends_to_local_content() {
        let mut storage = InMemoryStorage::new();
        storage.write("notes/today.txt", b"hello").unwrap();

        let hash = storage.append("notes/today.txt", b" world").unwrap();

        assert_eq!(hash, ContentHash::of(b"hello world"));
        assert_eq!(
            storage.read_file("notes/today.txt").unwrap(),
            b"hello world"
        );
    }

    #[test]
    fn passthrough_content_is_not_read_locally() {
        let mut storage = InMemoryStorage::new();
        storage.register_passthrough("remote.bin").unwrap();

        assert_eq!(
            storage.read_file("remote.bin"),
            Err(Error::ContentUnavailable)
        );
    }

    #[test]
    fn writes_replace_passthrough_markers() {
        let mut storage = InMemoryStorage::new();
        storage.register_passthrough("remote.bin").unwrap();
        storage.write("remote.bin", b"local").unwrap();

        assert_eq!(storage.read_file("remote.bin").unwrap(), b"local");
    }

    #[test]
    fn streams_content_in_order() {
        let mut storage = InMemoryStorage::new();
        storage.write("data", b"abcdef").unwrap();
        let mut stream = storage.stream("data", 2).unwrap();
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
    fn rejects_unsafe_paths_and_zero_size_chunks() {
        let mut storage = InMemoryStorage::new();

        assert_eq!(
            storage.write("../secret", b"content"),
            Err(Error::InvalidPath)
        );
        storage.write("safe", b"content").unwrap();
        assert!(matches!(
            storage.stream("safe", 0),
            Err(Error::InvalidChunkSize)
        ));
    }
}
