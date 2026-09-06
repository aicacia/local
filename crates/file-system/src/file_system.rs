use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;

use automerge::{
    AutoCommit, ObjType, ROOT, ReadDoc, ScalarValue, Value,
    sync::{Message, State, SyncDoc},
    transaction::Transactable,
};
use tokio::sync::mpsc;

use crate::{ChunkStream, ContentHash, FileEntry, PeerCodec, Storage};

const PROTOCOL_VERSION: u8 = 1;
const REQUEST_CAPACITY: usize = 64;

type MetadataResult<C> =
    Result<Vec<FileEntry<<C as PeerCodec>::PeerId>>, SyncError<(), <C as PeerCodec>::Error>>;

#[derive(Debug)]
pub enum SyncRequest<P> {
    Broadcast(Vec<u8>),
    Send { peer: P, data: Vec<u8> },
}

#[derive(Debug)]
pub enum SyncError<StorageError, PeerError> {
    Storage(StorageError),
    Peer(PeerError),
    InvalidMessage,
    Metadata(automerge::AutomergeError),
}

impl<StorageError: fmt::Display, PeerError: fmt::Display> fmt::Display
    for SyncError<StorageError, PeerError>
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => error.fmt(formatter),
            Self::Peer(error) => error.fmt(formatter),
            Self::InvalidMessage => formatter.write_str("invalid sync message"),
            Self::Metadata(error) => error.fmt(formatter),
        }
    }
}

#[must_use]
pub struct FileSystem<S: Storage> {
    storage: S,
    documents: BTreeMap<String, AutoCommit>,
    broadcast_states: BTreeMap<String, State>,
    peer_states: BTreeMap<(S::PeerId, String), State>,
    sync_requests: Option<mpsc::Receiver<SyncRequest<S::PeerId>>>,
    sync_sender: mpsc::Sender<SyncRequest<S::PeerId>>,
}

impl<S: Storage> FileSystem<S> {
    pub fn new(storage: S) -> Self {
        let (sync_sender, sync_requests) = mpsc::channel(REQUEST_CAPACITY);
        Self {
            storage,
            documents: BTreeMap::new(),
            broadcast_states: BTreeMap::new(),
            peer_states: BTreeMap::new(),
            sync_requests: Some(sync_requests),
            sync_sender,
        }
    }

    pub fn take_sync_requests(&mut self) -> Option<mpsc::Receiver<SyncRequest<S::PeerId>>> {
        self.sync_requests.take()
    }

    pub fn storage(&self) -> &S {
        &self.storage
    }

    pub fn storage_mut(&mut self) -> &mut S {
        &mut self.storage
    }

    pub fn into_storage(self) -> S {
        self.storage
    }

    pub fn write(&mut self, path: &str, content: &[u8]) -> Result<FileEntry<S::PeerId>, S::Error> {
        let entry = self.storage.write(path, content)?;
        let (folder, _) = split_path(path).expect("storage accepted an invalid path");
        let message = {
            let document = self.documents.entry(folder.to_string()).or_default();
            store_entry::<S::PeerCodec>(document, &entry)
                .expect("metadata generated from a valid entry");
            let state = self.broadcast_states.entry(folder.to_string()).or_default();
            document
                .sync()
                .generate_sync_message(state)
                .map(|message| encode_envelope(folder, message.encode()))
        };
        if let Some(data) = message {
            let _ = self.emit(SyncRequest::Broadcast(data));
        }
        Ok(entry)
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

    pub fn receive(
        &mut self,
        peer: S::PeerId,
        data: Vec<u8>,
    ) -> Result<(), SyncError<S::Error, <S::PeerCodec as PeerCodec>::Error>> {
        let (folder, message) = decode_envelope(&data)?;
        let message = Message::decode(&message).map_err(|_| SyncError::InvalidMessage)?;
        let (entries, reply) = {
            let document = self.documents.entry(folder.clone()).or_default();
            let state = self
                .peer_states
                .entry((peer.clone(), folder.clone()))
                .or_default();
            document
                .sync()
                .receive_sync_message(state, message)
                .map_err(SyncError::Metadata)?;
            let entries = load_entries::<S::PeerCodec>(document).map_err(map_metadata_error)?;
            let reply = document
                .sync()
                .generate_sync_message(state)
                .map(|message| encode_envelope(&folder, message.encode()));
            (entries, reply)
        };

        for entry in entries {
            let path = join_path(&folder, &entry.name);
            if self.storage.entry(&path).is_err() {
                self.storage
                    .register_passthrough(&path, entry.hash, entry.size, entry.providers)
                    .map_err(SyncError::Storage)?;
            }
        }

        if let Some(data) = reply {
            self.emit(SyncRequest::Send { peer, data })?;
        }
        Ok(())
    }

    fn emit(
        &self,
        request: SyncRequest<S::PeerId>,
    ) -> Result<(), SyncError<S::Error, <S::PeerCodec as PeerCodec>::Error>> {
        self.sync_sender
            .try_send(request)
            .map_err(|_| SyncError::InvalidMessage)
    }
}

fn store_entry<C: PeerCodec>(
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
    Ok(())
}

fn load_entries<C: PeerCodec>(document: &AutoCommit) -> MetadataResult<C> {
    document
        .keys(ROOT)
        .map(|name| load_entry::<C>(document, &name))
        .collect()
}

fn load_entry<C: PeerCodec>(
    document: &AutoCommit,
    name: &str,
) -> Result<FileEntry<C::PeerId>, SyncError<(), C::Error>> {
    let Some((Value::Object(ObjType::Map), object)) =
        document.get(ROOT, name).map_err(SyncError::Metadata)?
    else {
        return Err(SyncError::InvalidMessage);
    };
    let hash = bytes::<C>(document, &object, "hash")?;
    let hash = hash.try_into().map_err(|_| SyncError::InvalidMessage)?;
    let size = uint::<C>(document, &object, "size")?;
    let providers = decode_providers::<C>(&bytes::<C>(document, &object, "providers")?)?;
    Ok(FileEntry::new(
        name.to_string(),
        ContentHash::from_bytes(hash),
        size,
        providers,
        false,
    ))
}

fn bytes<C: PeerCodec>(
    document: &AutoCommit,
    object: &automerge::ObjId,
    key: &str,
) -> Result<Vec<u8>, SyncError<(), C::Error>> {
    match document.get(object, key).map_err(SyncError::Metadata)? {
        Some((Value::Scalar(value), _)) => match value.as_ref() {
            ScalarValue::Bytes(bytes) => Ok(bytes.clone()),
            _ => Err(SyncError::InvalidMessage),
        },
        _ => Err(SyncError::InvalidMessage),
    }
}

fn uint<C: PeerCodec>(
    document: &AutoCommit,
    object: &automerge::ObjId,
    key: &str,
) -> Result<u64, SyncError<(), C::Error>> {
    match document.get(object, key).map_err(SyncError::Metadata)? {
        Some((Value::Scalar(value), _)) => match value.as_ref() {
            ScalarValue::Uint(value) => Ok(*value),
            _ => Err(SyncError::InvalidMessage),
        },
        _ => Err(SyncError::InvalidMessage),
    }
}

fn encode_providers<C: PeerCodec>(providers: &BTreeSet<C::PeerId>) -> Vec<u8> {
    let mut bytes = Vec::new();
    for peer in providers {
        let peer = C::encode(peer);
        let length = u32::try_from(peer.len()).expect("provider encoding exceeds u32");
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.extend_from_slice(&peer);
    }
    bytes
}

fn decode_providers<C: PeerCodec>(
    mut bytes: &[u8],
) -> Result<BTreeSet<C::PeerId>, SyncError<(), C::Error>> {
    let mut providers = BTreeSet::new();
    while !bytes.is_empty() {
        let length = bytes
            .get(..4)
            .ok_or(SyncError::InvalidMessage)?
            .try_into()
            .map_err(|_| SyncError::InvalidMessage)?;
        let length =
            usize::try_from(u32::from_be_bytes(length)).map_err(|_| SyncError::InvalidMessage)?;
        bytes = &bytes[4..];
        let peer = bytes.get(..length).ok_or(SyncError::InvalidMessage)?;
        providers.insert(C::decode(peer).map_err(SyncError::Peer)?);
        bytes = &bytes[length..];
    }
    Ok(providers)
}

fn map_metadata_error<StorageError, PeerError>(
    error: SyncError<(), PeerError>,
) -> SyncError<StorageError, PeerError> {
    match error {
        SyncError::Peer(error) => SyncError::Peer(error),
        SyncError::InvalidMessage => SyncError::InvalidMessage,
        SyncError::Metadata(error) => SyncError::Metadata(error),
        SyncError::Storage(()) => unreachable!(),
    }
}

fn encode_envelope(folder: &str, message: Vec<u8>) -> Vec<u8> {
    let folder = folder.as_bytes();
    let length = u32::try_from(folder.len()).expect("folder name exceeds u32");
    let mut data = Vec::with_capacity(5 + folder.len() + message.len());
    data.push(PROTOCOL_VERSION);
    data.extend_from_slice(&length.to_be_bytes());
    data.extend_from_slice(folder);
    data.extend_from_slice(&message);
    data
}

fn decode_envelope<StorageError, PeerError>(
    data: &[u8],
) -> Result<(String, Vec<u8>), SyncError<StorageError, PeerError>> {
    let version = *data.first().ok_or(SyncError::InvalidMessage)?;
    if version != PROTOCOL_VERSION {
        return Err(SyncError::InvalidMessage);
    }
    let length: [u8; 4] = data
        .get(1..5)
        .ok_or(SyncError::InvalidMessage)?
        .try_into()
        .map_err(|_| SyncError::InvalidMessage)?;
    let length =
        usize::try_from(u32::from_be_bytes(length)).map_err(|_| SyncError::InvalidMessage)?;
    let end = 5usize
        .checked_add(length)
        .ok_or(SyncError::InvalidMessage)?;
    let folder = core::str::from_utf8(data.get(5..end).ok_or(SyncError::InvalidMessage)?)
        .map_err(|_| SyncError::InvalidMessage)?;
    validate_folder(folder).map_err(|_| SyncError::InvalidMessage)?;
    let message = data.get(end..).ok_or(SyncError::InvalidMessage)?;
    if message.is_empty() {
        return Err(SyncError::InvalidMessage);
    }
    Ok((folder.to_string(), message.to_vec()))
}

fn split_path(path: &str) -> Result<(&str, &str), ()> {
    if path.is_empty() || path.starts_with('/') || path.ends_with('/') {
        return Err(());
    }
    let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
    validate_folder(folder)?;
    if !is_name(name) {
        return Err(());
    }
    Ok((folder, name))
}

fn join_path(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_string()
    } else {
        alloc::format!("{folder}/{name}")
    }
}

fn validate_folder(folder: &str) -> Result<(), ()> {
    if folder.is_empty() || folder.split('/').all(is_name) {
        Ok(())
    } else {
        Err(())
    }
}

fn is_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".."
}
