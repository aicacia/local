use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

use automerge::{
    AutoCommit, ObjType, ROOT, ReadDoc, ScalarValue, Value, transaction::Transactable,
};

use crate::{ChunkStream, ContentHash, FileEntry, MergeStrategy, PeerId};

const BLOBS_DIRECTORY: &str = ".lidp/blobs";
const METADATA_DIRECTORY: &str = ".lidp/metadata";
const ROOT_DOCUMENT: &str = "root.automerge";

#[derive(Debug)]
pub struct NativeFileSystem {
    root: PathBuf,
    local_peer: PeerId,
}

impl NativeFileSystem {
    pub fn new(root: impl AsRef<Path>, local_peer: PeerId) -> io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join(BLOBS_DIRECTORY))?;
        fs::create_dir_all(root.join(METADATA_DIRECTORY))?;
        Ok(Self { root, local_peer })
    }

    pub fn write(&self, path: &str, content: &[u8]) -> io::Result<FileEntry> {
        let (folder, name) = split_path(path)?;
        let hash = ContentHash::of(content);
        let blob_path = self.blob_path(hash);
        if !blob_path.exists() {
            fs::write(blob_path, content)?;
        }
        let size = u64::try_from(content.len()).map_err(|_| invalid_data("content exceeds u64"))?;
        let mut providers = BTreeSet::new();
        providers.insert(self.local_peer);
        let entry = FileEntry::new(name.into(), hash, size, providers, true);
        self.store_entry(folder, &entry)?;
        Ok(entry)
    }

    pub fn entry(&self, path: &str) -> io::Result<FileEntry> {
        let (folder, name) = split_path(path)?;
        self.load_entry(folder, name)
    }

    pub fn list(&self, folder: &str) -> io::Result<Vec<FileEntry>> {
        validate_folder(folder)?;
        let document = self.load_document(folder)?;
        document
            .keys(ROOT)
            .map(|name| self.load_entry_from_document(&document, &name))
            .collect()
    }

    pub fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let entry = self.entry(path)?;
        if !entry.local {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "file content is not stored locally",
            ));
        }
        let content = fs::read(self.blob_path(entry.hash))?;
        if ContentHash::of(&content) != entry.hash {
            return Err(invalid_data("stored content does not match its hash"));
        }
        Ok(content)
    }

    pub fn stream(&self, path: &str, chunk_size: usize) -> io::Result<ChunkStream> {
        if chunk_size == 0 {
            return Err(invalid_input("chunk size must not be zero"));
        }
        Ok(ChunkStream::new(self.read(path)?, chunk_size))
    }

    pub fn metadata_path(&self, folder: &str) -> io::Result<PathBuf> {
        validate_folder(folder)?;
        let mut path = self.root.join(METADATA_DIRECTORY);
        if folder.is_empty() {
            path.push(ROOT_DOCUMENT);
        } else {
            path.push(folder);
            path.set_extension("automerge");
        }
        Ok(path)
    }

    fn blob_path(&self, hash: ContentHash) -> PathBuf {
        self.root.join(BLOBS_DIRECTORY).join(hash.to_string())
    }

    fn store_entry(&self, folder: &str, entry: &FileEntry) -> io::Result<()> {
        let mut document = self.load_document(folder)?;
        let object = match document.get(ROOT, &entry.name).map_err(metadata_error)? {
            Some((Value::Object(ObjType::Map), object)) => object,
            Some(_) => {
                document.delete(ROOT, &entry.name).map_err(metadata_error)?;
                document
                    .put_object(ROOT, &entry.name, ObjType::Map)
                    .map_err(metadata_error)?
            }
            None => document
                .put_object(ROOT, &entry.name, ObjType::Map)
                .map_err(metadata_error)?,
        };
        document
            .put(&object, "hash", entry.hash.as_bytes().to_vec())
            .map_err(metadata_error)?;
        document
            .put(&object, "size", entry.size)
            .map_err(metadata_error)?;
        document
            .put(&object, "local", entry.local)
            .map_err(metadata_error)?;
        document
            .put(
                &object,
                "merge_strategy",
                merge_strategy_name(entry.merge_strategy),
            )
            .map_err(metadata_error)?;
        document
            .put(&object, "providers", encode_providers(&entry.providers))
            .map_err(metadata_error)?;
        self.save_document(folder, &mut document)
    }

    fn load_entry(&self, folder: &str, name: &str) -> io::Result<FileEntry> {
        let document = self.load_document(folder)?;
        self.load_entry_from_document(&document, name)
    }

    fn load_entry_from_document(&self, document: &AutoCommit, name: &str) -> io::Result<FileEntry> {
        let Some((Value::Object(ObjType::Map), object)) =
            document.get(ROOT, name).map_err(metadata_error)?
        else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "file entry was not found",
            ));
        };
        let hash = scalar_bytes(document, &object, "hash")?;
        let hash: [u8; blake3::OUT_LEN] = hash
            .try_into()
            .map_err(|_| invalid_data("invalid content hash"))?;
        let size = scalar_uint(document, &object, "size")?;
        let local = scalar_bool(document, &object, "local")?;
        let providers = decode_providers(&scalar_bytes(document, &object, "providers")?)?;
        let strategy = scalar_string(document, &object, "merge_strategy")?;
        let merge_strategy = match strategy.as_str() {
            "lww" => MergeStrategy::Lww,
            "automerge-document" => MergeStrategy::AutomergeDocument,
            _ => return Err(invalid_data("unknown merge strategy")),
        };
        Ok(FileEntry {
            name: name.into(),
            hash: ContentHash::from_bytes(hash),
            size,
            providers,
            local,
            merge_strategy,
        })
    }

    fn load_document(&self, folder: &str) -> io::Result<AutoCommit> {
        let path = self.metadata_path(folder)?;
        match fs::read(path) {
            Ok(bytes) => AutoCommit::load(&bytes).map_err(metadata_error),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(AutoCommit::new()),
            Err(error) => Err(error),
        }
    }

    fn save_document(&self, folder: &str, document: &mut AutoCommit) -> io::Result<()> {
        let path = self.metadata_path(folder)?;
        let parent = path
            .parent()
            .ok_or_else(|| invalid_data("metadata path has no parent"))?;
        fs::create_dir_all(parent)?;
        fs::write(path, document.save())
    }
}

fn scalar_bytes(
    document: &AutoCommit,
    object: &automerge::ObjId,
    key: &str,
) -> io::Result<Vec<u8>> {
    match document.get(object, key).map_err(metadata_error)? {
        Some((Value::Scalar(value), _)) => match value.as_ref() {
            ScalarValue::Bytes(value) => Ok(value.clone()),
            _ => Err(invalid_data("metadata field is not bytes")),
        },
        _ => Err(invalid_data("metadata field is missing")),
    }
}

fn scalar_uint(document: &AutoCommit, object: &automerge::ObjId, key: &str) -> io::Result<u64> {
    match document.get(object, key).map_err(metadata_error)? {
        Some((Value::Scalar(value), _)) => match value.as_ref() {
            ScalarValue::Uint(value) => Ok(*value),
            _ => Err(invalid_data("metadata field is not an unsigned integer")),
        },
        _ => Err(invalid_data("metadata field is missing")),
    }
}

fn scalar_bool(document: &AutoCommit, object: &automerge::ObjId, key: &str) -> io::Result<bool> {
    match document.get(object, key).map_err(metadata_error)? {
        Some((Value::Scalar(value), _)) => match value.as_ref() {
            ScalarValue::Boolean(value) => Ok(*value),
            _ => Err(invalid_data("metadata field is not a boolean")),
        },
        _ => Err(invalid_data("metadata field is missing")),
    }
}

fn scalar_string(
    document: &AutoCommit,
    object: &automerge::ObjId,
    key: &str,
) -> io::Result<String> {
    match document.get(object, key).map_err(metadata_error)? {
        Some((Value::Scalar(value), _)) => match value.as_ref() {
            ScalarValue::Str(value) => Ok(value.to_string()),
            _ => Err(invalid_data("metadata field is not a string")),
        },
        _ => Err(invalid_data("metadata field is missing")),
    }
}

fn encode_providers(providers: &BTreeSet<PeerId>) -> Vec<u8> {
    providers.iter().flat_map(|peer| peer.0).collect()
}

fn decode_providers(bytes: &[u8]) -> io::Result<BTreeSet<PeerId>> {
    let (chunks, remainder) = bytes.as_chunks::<32>();
    if !remainder.is_empty() {
        return Err(invalid_data("invalid provider list"));
    }
    Ok(chunks.iter().copied().map(PeerId).collect())
}

fn merge_strategy_name(strategy: MergeStrategy) -> &'static str {
    match strategy {
        MergeStrategy::Lww => "lww",
        MergeStrategy::AutomergeDocument => "automerge-document",
    }
}

fn split_path(path: &str) -> io::Result<(&str, &str)> {
    if path.is_empty() || path.starts_with('/') || path.ends_with('/') {
        return Err(invalid_input("path must name a file within a folder"));
    }
    let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
    validate_folder(folder)?;
    if !is_name(name) {
        return Err(invalid_input("path must name a file within a folder"));
    }
    Ok((folder, name))
}

fn validate_folder(folder: &str) -> io::Result<()> {
    if folder.is_empty() || folder.split('/').all(is_name) {
        Ok(())
    } else {
        Err(invalid_input("folder contains an invalid path component"))
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

fn metadata_error(error: automerge::AutomergeError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use super::NativeFileSystem;
    use crate::PeerId;

    #[test]
    fn persists_blobs_and_folder_metadata() {
        let root = env::temp_dir().join(format!("file-system-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let peer = PeerId([4; 32]);
        let file_system = NativeFileSystem::new(&root, peer).unwrap();
        let written = file_system.write("notes/today.txt", b"hello").unwrap();
        let metadata_path = file_system.metadata_path("notes").unwrap();

        assert!(metadata_path.exists());
        assert_eq!(file_system.read("notes/today.txt").unwrap(), b"hello");
        drop(file_system);

        let reopened = NativeFileSystem::new(&root, peer).unwrap();
        assert_eq!(reopened.entry("notes/today.txt").unwrap(), written);
        let _ = fs::remove_dir_all(root);
    }
}
