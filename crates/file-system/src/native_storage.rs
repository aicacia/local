use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::{ChunkStream, ContentHash, Storage};

const BLOBS_DIRECTORY: &str = ".lidp/blobs";
const PATHS_DIRECTORY: &str = ".lidp/paths";
const PASSTHROUGH_DIRECTORY: &str = ".lidp/passthrough";

#[derive(Debug)]
pub struct NativeStorage {
    root: PathBuf,
}

impl NativeStorage {
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join(BLOBS_DIRECTORY))?;
        fs::create_dir_all(root.join(PATHS_DIRECTORY))?;
        fs::create_dir_all(root.join(PASSTHROUGH_DIRECTORY))?;
        Ok(Self { root })
    }

    pub fn write(&mut self, path: &str, content: &[u8]) -> io::Result<ContentHash> {
        validate_path(path)?;
        let hash = ContentHash::of(content);
        let blob_path = self.blob_path(hash);
        if !blob_path.exists() {
            fs::write(blob_path, content)?;
        }
        self.write_path_hash(path, hash)?;
        let _ = fs::remove_file(self.passthrough_path(path));
        Ok(hash)
    }

    pub fn append(&mut self, path: &str, content: &[u8]) -> io::Result<ContentHash> {
        let mut existing = self.read_file(path)?;
        existing.extend_from_slice(content);
        self.write(path, &existing)
    }

    pub fn register_passthrough(&mut self, path: &str) -> io::Result<()> {
        validate_path(path)?;
        let marker = self.passthrough_path(path);
        let parent = marker
            .parent()
            .ok_or_else(|| invalid_data("passthrough path has no parent"))?;
        fs::create_dir_all(parent)?;
        fs::write(marker, [])?;
        let _ = fs::remove_file(self.path_hash_path(path));
        Ok(())
    }

    pub fn read_file(&self, path: &str) -> io::Result<Vec<u8>> {
        validate_path(path)?;
        let hash_path = self.path_hash_path(path);
        let bytes = match fs::read(hash_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if self.passthrough_path(path).exists() {
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "file content is not stored locally",
                    ));
                }
                return Err(error);
            }
            Err(error) => return Err(error),
        };
        let hash = content_hash(&bytes)?;
        let content = fs::read(self.blob_path(hash))?;
        if ContentHash::of(&content) != hash {
            return Err(invalid_data("stored content does not match its hash"));
        }
        Ok(content)
    }

    pub fn stream(&self, path: &str, chunk_size: usize) -> io::Result<ChunkStream> {
        if chunk_size == 0 {
            return Err(invalid_input("chunk size must not be zero"));
        }
        Ok(ChunkStream::new(self.read_file(path)?, chunk_size))
    }

    fn blob_path(&self, hash: ContentHash) -> PathBuf {
        self.root.join(BLOBS_DIRECTORY).join(hash.to_string())
    }

    fn path_hash_path(&self, path: &str) -> PathBuf {
        self.root.join(PATHS_DIRECTORY).join(path)
    }

    fn passthrough_path(&self, path: &str) -> PathBuf {
        self.root.join(PASSTHROUGH_DIRECTORY).join(path)
    }

    fn write_path_hash(&self, path: &str, hash: ContentHash) -> io::Result<()> {
        let path = self.path_hash_path(path);
        let parent = path
            .parent()
            .ok_or_else(|| invalid_data("path metadata has no parent"))?;
        fs::create_dir_all(parent)?;
        fs::write(path, hash.as_bytes())
    }
}

impl Storage for NativeStorage {
    type Error = io::Error;

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

fn content_hash(bytes: &[u8]) -> io::Result<ContentHash> {
    let hash: [u8; blake3::OUT_LEN] = bytes
        .try_into()
        .map_err(|_| invalid_data("invalid content hash"))?;
    Ok(ContentHash::from_bytes(hash))
}

fn validate_path(path: &str) -> io::Result<()> {
    if path.is_empty() || path.starts_with('/') || path.ends_with('/') {
        return Err(invalid_input("path must name a file within a folder"));
    }
    if path.split('/').all(is_name) {
        Ok(())
    } else {
        Err(invalid_input("path contains an invalid component"))
    }
}

fn is_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".."
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn invalid_input(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use super::NativeStorage;
    use crate::ContentHash;

    #[test]
    fn persists_local_content_by_path() {
        let root = env::temp_dir().join(format!("file-system-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut storage = NativeStorage::new(&root).unwrap();
        assert_eq!(
            storage.write("notes/today.txt", b"hello").unwrap(),
            ContentHash::of(b"hello")
        );
        drop(storage);

        let storage = NativeStorage::new(&root).unwrap();
        assert_eq!(storage.read_file("notes/today.txt").unwrap(), b"hello");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn appends_to_local_content() {
        let root = env::temp_dir().join(format!("file-system-{}-append", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let mut storage = NativeStorage::new(&root).unwrap();
        storage.write("notes/today.txt", b"hello").unwrap();

        let hash = storage.append("notes/today.txt", b" world").unwrap();

        assert_eq!(hash, ContentHash::of(b"hello world"));
        assert_eq!(
            storage.read_file("notes/today.txt").unwrap(),
            b"hello world"
        );
        let _ = fs::remove_dir_all(root);
    }
}
