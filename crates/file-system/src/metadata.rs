use alloc::{
    collections::{BTreeMap, BTreeSet},
    format,
    string::{String, ToString},
    vec::Vec,
};

use automerge::{
    AutoCommit, ObjType, ROOT, ReadDoc, ScalarValue, Value, transaction::Transactable,
};

use crate::{ContentHash, PeerCodec};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MergeStrategy {
    #[default]
    Lww,
    AutomergeDocument,
}

impl MergeStrategy {
    #[must_use]
    pub fn for_name(name: &str) -> Self {
        match name.rsplit_once('.') {
            Some((_, "automerge" | "am")) => Self::AutomergeDocument,
            _ => Self::Lww,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileEntry<P> {
    pub name: String,
    pub hash: ContentHash,
    pub size: u64,
    pub providers: BTreeSet<P>,
    pub local: bool,
    pub tombstoned: bool,
    pub merge_strategy: MergeStrategy,
}

impl<P> FileEntry<P> {
    #[must_use]
    pub fn new(
        name: String,
        hash: ContentHash,
        size: u64,
        providers: BTreeSet<P>,
        local: bool,
    ) -> Self {
        let merge_strategy = MergeStrategy::for_name(&name);
        Self {
            name,
            hash,
            size,
            providers,
            local,
            tombstoned: false,
            merge_strategy,
        }
    }
}

#[derive(Debug)]
pub(crate) enum MetadataError<E> {
    Peer(E),
    InvalidMessage,
    Metadata(automerge::AutomergeError),
}

pub(crate) fn store_entry<C: PeerCodec>(
    document: &mut AutoCommit,
    entry: &FileEntry<C::PeerId>,
) -> Result<(), automerge::AutomergeError> {
    let object = match document.get(ROOT, &entry.name)? {
        Some((Value::Object(ObjType::Map), object)) => object,
        Some(_) => {
            document.delete(ROOT, &entry.name)?;
            document.put_object(ROOT, &entry.name, ObjType::Map)?
        }
        None => document.put_object(ROOT, &entry.name, ObjType::Map)?,
    };
    document.put(&object, "hash", entry.hash.as_bytes().to_vec())?;
    document.put(&object, "size", entry.size)?;
    document.put(
        &object,
        "providers",
        encode_providers::<C>(&entry.providers),
    )?;
    document.put(&object, "tombstoned", false)?;
    Ok(())
}

pub(crate) fn store_tombstone(
    document: &mut AutoCommit,
    name: &str,
) -> Result<(), automerge::AutomergeError> {
    let Some((Value::Object(ObjType::Map), object)) = document.get(ROOT, name)? else {
        return Ok(());
    };
    document.put(&object, "tombstoned", true)
}

pub(crate) fn load_entries<C: PeerCodec>(
    document: &AutoCommit,
) -> Result<BTreeMap<String, FileEntry<C::PeerId>>, MetadataError<C::Error>> {
    let mut entries = BTreeMap::new();
    for name in document.keys(ROOT) {
        let entry = load_entry::<C>(document, &name)?;
        entries.insert(name, entry);
    }
    Ok(entries)
}

pub(crate) fn load_entry<C: PeerCodec>(
    document: &AutoCommit,
    name: &str,
) -> Result<FileEntry<C::PeerId>, MetadataError<C::Error>> {
    let Some((Value::Object(ObjType::Map), object)) =
        document.get(ROOT, name).map_err(MetadataError::Metadata)?
    else {
        return Err(MetadataError::InvalidMessage);
    };
    let hash = bytes(document, &object, "hash")?;
    let hash: [u8; 32] = hash.try_into().map_err(|_| MetadataError::InvalidMessage)?;
    let size = uint(document, &object, "size")?;
    let providers = decode_providers::<C>(&bytes(document, &object, "providers")?)?;
    let mut entry = FileEntry::new(
        name.to_string(),
        ContentHash::from_bytes(hash),
        size,
        providers,
        false,
    );
    entry.tombstoned = bool_value(document, &object, "tombstoned")?.unwrap_or(false);
    Ok(entry)
}

fn bool_value<E>(
    document: &AutoCommit,
    object: &automerge::ObjId,
    key: &str,
) -> Result<Option<bool>, MetadataError<E>> {
    match document.get(object, key).map_err(MetadataError::Metadata)? {
        Some((Value::Scalar(value), _)) => match value.as_ref() {
            ScalarValue::Boolean(value) => Ok(Some(*value)),
            _ => Err(MetadataError::InvalidMessage),
        },
        None => Ok(None),
        _ => Err(MetadataError::InvalidMessage),
    }
}

fn bytes<E>(
    document: &AutoCommit,
    object: &automerge::ObjId,
    key: &str,
) -> Result<Vec<u8>, MetadataError<E>> {
    match document.get(object, key).map_err(MetadataError::Metadata)? {
        Some((Value::Scalar(value), _)) => match value.as_ref() {
            ScalarValue::Bytes(bytes) => Ok(bytes.clone()),
            _ => Err(MetadataError::InvalidMessage),
        },
        _ => Err(MetadataError::InvalidMessage),
    }
}

fn uint<E>(
    document: &AutoCommit,
    object: &automerge::ObjId,
    key: &str,
) -> Result<u64, MetadataError<E>> {
    match document.get(object, key).map_err(MetadataError::Metadata)? {
        Some((Value::Scalar(value), _)) => match value.as_ref() {
            ScalarValue::Uint(value) => Ok(*value),
            _ => Err(MetadataError::InvalidMessage),
        },
        _ => Err(MetadataError::InvalidMessage),
    }
}

pub(crate) fn encode_providers<C: PeerCodec>(providers: &BTreeSet<C::PeerId>) -> Vec<u8> {
    let mut bytes = Vec::new();
    for peer in providers {
        let peer = C::encode(peer);
        let length = u32::try_from(peer.len()).expect("provider encoding exceeds u32");
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.extend_from_slice(&peer);
    }
    bytes
}

pub(crate) fn decode_providers<C: PeerCodec>(
    mut bytes: &[u8],
) -> Result<BTreeSet<C::PeerId>, MetadataError<C::Error>> {
    let mut providers = BTreeSet::new();
    while !bytes.is_empty() {
        let length = bytes
            .get(..4)
            .ok_or(MetadataError::InvalidMessage)?
            .try_into()
            .map_err(|_| MetadataError::InvalidMessage)?;
        let length = usize::try_from(u32::from_be_bytes(length))
            .map_err(|_| MetadataError::InvalidMessage)?;
        bytes = &bytes[4..];
        let peer = bytes.get(..length).ok_or(MetadataError::InvalidMessage)?;
        providers.insert(C::decode(peer).map_err(MetadataError::Peer)?);
        bytes = &bytes[length..];
    }
    Ok(providers)
}

pub(crate) fn split_path(path: &str) -> Result<(&str, &str), ()> {
    if path.is_empty() || path.starts_with('/') || path.ends_with('/') {
        return Err(());
    }
    let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
    validate_folder(folder)?;
    if folder == ".blobs"
        || folder.starts_with(".blobs/")
        || folder == ".paths"
        || folder.starts_with(".paths/")
        || folder == ".passthrough"
        || folder.starts_with(".passthrough/")
        || folder == ".sync"
        || folder.starts_with(".sync/")
    {
        return Err(());
    }
    if !is_name(name) {
        return Err(());
    }
    Ok((folder, name))
}

pub(crate) fn join_path(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_string()
    } else {
        format!("{folder}/{name}")
    }
}

pub(crate) fn validate_folder(folder: &str) -> Result<(), ()> {
    if folder.is_empty() || folder.split('/').all(is_name) {
        Ok(())
    } else {
        Err(())
    }
}

pub(crate) fn is_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".."
}
