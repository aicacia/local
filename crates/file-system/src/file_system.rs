use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec::Vec,
};
use core::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use automerge::{
    AutoCommit, ObjType, ROOT, ReadDoc, ScalarValue, Value,
    sync::{Message, State, SyncDoc},
    transaction::Transactable,
};
use futures_core::Stream;
use tokio::sync::{mpsc, oneshot};

use crate::{ChunkStream, ContentHash, FileEntry, PeerCodec, Storage};

const PROTOCOL_VERSION: u8 = 1;
const METADATA_MESSAGE: u8 = 0;
const BLOB_REQUEST: u8 = 1;
const BLOB_RESPONSE: u8 = 2;
const REQUEST_CAPACITY: usize = 64;
const READ_CHUNK_SIZE: usize = 64 * 1024;

type MetadataResult<C> =
    Result<Vec<FileEntry<<C as PeerCodec>::PeerId>>, SyncError<(), <C as PeerCodec>::Error>>;
type PendingReads<S> = BTreeMap<u64, PendingRead<<S as Storage>::Error, <S as Storage>::PeerId>>;
type PendingStreams<S> =
    BTreeMap<u64, PendingStream<<S as Storage>::Error, <S as Storage>::PeerId>>;
type BlobRequest = (u64, String, ContentHash, u64, usize);
type BlobResponse = (u64, ContentHash, u64, bool, ContentHash, Vec<u8>);

struct PendingRead<StorageError, PeerId> {
    hash: ContentHash,
    size: u64,
    offset: u64,
    path: String,
    peer: PeerId,
    content: Vec<u8>,
    sender: oneshot::Sender<Result<Vec<u8>, ReadError<StorageError>>>,
}

struct PendingStream<StorageError, PeerId> {
    hash: ContentHash,
    size: u64,
    offset: u64,
    path: String,
    chunk_size: usize,
    peer: PeerId,
    hasher: blake3::Hasher,
    sender: mpsc::UnboundedSender<Result<Vec<u8>, ReadError<StorageError>>>,
}

#[derive(Debug)]
pub enum SyncRequest<P> {
    Broadcast(Vec<u8>),
    Send { peer: P, data: Vec<u8> },
}

#[derive(Debug, Eq, PartialEq)]
pub enum ReadError<StorageError> {
    Storage(StorageError),
    NoProvider,
    RequestQueueFull,
    ResponseDropped,
    CorruptContent,
    InvalidChunkSize,
}

impl<StorageError: fmt::Display> fmt::Display for ReadError<StorageError> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => error.fmt(formatter),
            Self::NoProvider => formatter.write_str("file has no content provider"),
            Self::RequestQueueFull => formatter.write_str("sync request queue is full"),
            Self::ResponseDropped => formatter.write_str("content provider did not respond"),
            Self::CorruptContent => {
                formatter.write_str("received content does not match its metadata")
            }
            Self::InvalidChunkSize => formatter.write_str("chunk size must not be zero"),
        }
    }
}

pub struct ReadFuture<StorageError>(oneshot::Receiver<Result<Vec<u8>, ReadError<StorageError>>>);

impl<StorageError> Future for ReadFuture<StorageError> {
    type Output = Result<Vec<u8>, ReadError<StorageError>>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        match Pin::new(&mut self.0).poll(context) {
            Poll::Ready(Ok(result)) => Poll::Ready(result),
            Poll::Ready(Err(_)) => Poll::Ready(Err(ReadError::ResponseDropped)),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct ReadStream<StorageError> {
    receiver: mpsc::UnboundedReceiver<Result<Vec<u8>, ReadError<StorageError>>>,
}

impl<StorageError> Stream for ReadStream<StorageError> {
    type Item = Result<Vec<u8>, ReadError<StorageError>>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.receiver.poll_recv(context)
    }
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
    pending_reads: PendingReads<S>,
    pending_streams: PendingStreams<S>,
    next_read_id: u64,
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
            pending_reads: BTreeMap::new(),
            pending_streams: BTreeMap::new(),
            next_read_id: 0,
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
        self.storage.read_file(path)
    }

    pub fn start_read(&mut self, path: &str) -> Result<ReadFuture<S::Error>, ReadError<S::Error>> {
        let entry = self.storage.entry(path).map_err(ReadError::Storage)?;
        let (sender, receiver) = oneshot::channel();
        if entry.local {
            let _ = sender.send(self.storage.read_file(path).map_err(ReadError::Storage));
            return Ok(ReadFuture(receiver));
        }
        let peer = entry
            .providers
            .into_iter()
            .next()
            .ok_or(ReadError::NoProvider)?;
        let id = self.next_read_id;
        self.next_read_id = self.next_read_id.wrapping_add(1);
        self.pending_reads.insert(
            id,
            PendingRead {
                hash: entry.hash,
                size: entry.size,
                offset: 0,
                path: path.to_string(),
                peer: peer.clone(),
                content: Vec::new(),
                sender,
            },
        );
        let data = encode_blob_request(id, path, entry.hash, 0, READ_CHUNK_SIZE);
        if self
            .sync_sender
            .try_send(SyncRequest::Send { peer, data })
            .is_err()
        {
            self.pending_reads.remove(&id);
            return Err(ReadError::RequestQueueFull);
        }
        Ok(ReadFuture(receiver))
    }

    pub fn stream(&self, path: &str, chunk_size: usize) -> Result<ChunkStream, S::Error> {
        self.storage.stream(path, chunk_size)
    }

    pub fn start_stream(
        &mut self,
        path: &str,
        chunk_size: usize,
    ) -> Result<ReadStream<S::Error>, ReadError<S::Error>> {
        if chunk_size == 0 {
            return Err(ReadError::InvalidChunkSize);
        }
        let entry = self.storage.entry(path).map_err(ReadError::Storage)?;
        let (sender, receiver) = mpsc::unbounded_channel();
        if entry.local {
            let content = self.storage.read_file(path).map_err(ReadError::Storage)?;
            for chunk in content.chunks(chunk_size) {
                let _ = sender.send(Ok(chunk.to_vec()));
            }
            return Ok(ReadStream { receiver });
        }
        let peer = entry
            .providers
            .into_iter()
            .next()
            .ok_or(ReadError::NoProvider)?;
        let id = self.next_read_id;
        self.next_read_id = self.next_read_id.wrapping_add(1);
        self.pending_streams.insert(
            id,
            PendingStream {
                hash: entry.hash,
                size: entry.size,
                offset: 0,
                path: path.to_string(),
                chunk_size,
                peer: peer.clone(),
                hasher: blake3::Hasher::new(),
                sender,
            },
        );
        let data = encode_blob_request(id, path, entry.hash, 0, chunk_size);
        if self
            .sync_sender
            .try_send(SyncRequest::Send { peer, data })
            .is_err()
        {
            self.pending_streams.remove(&id);
            return Err(ReadError::RequestQueueFull);
        }
        Ok(ReadStream { receiver })
    }

    pub fn receive(
        &mut self,
        peer: S::PeerId,
        data: Vec<u8>,
    ) -> Result<(), SyncError<S::Error, <S::PeerCodec as PeerCodec>::Error>> {
        match data.get(1).copied() {
            Some(BLOB_REQUEST) => return self.receive_blob_request(peer, &data),
            Some(BLOB_RESPONSE) => return self.receive_blob_response(&data),
            Some(METADATA_MESSAGE) => {}
            _ => return Err(SyncError::InvalidMessage),
        }
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

    fn receive_blob_request(
        &mut self,
        peer: S::PeerId,
        data: &[u8],
    ) -> Result<(), SyncError<S::Error, <S::PeerCodec as PeerCodec>::Error>> {
        let (id, path, hash, offset, chunk_size) = decode_blob_request(data)?;
        let content = self.storage.read_file(&path).map_err(SyncError::Storage)?;
        if ContentHash::of(&content) != hash {
            return Err(SyncError::InvalidMessage);
        }
        let offset = usize::try_from(offset).map_err(|_| SyncError::InvalidMessage)?;
        if offset > content.len() || chunk_size == 0 {
            return Err(SyncError::InvalidMessage);
        }
        let end = offset.saturating_add(chunk_size).min(content.len());
        self.emit(SyncRequest::Send {
            peer,
            data: encode_blob_response(
                id,
                hash,
                offset as u64,
                end == content.len(),
                content[offset..end].to_vec(),
            ),
        })
    }

    fn receive_blob_response(
        &mut self,
        data: &[u8],
    ) -> Result<(), SyncError<S::Error, <S::PeerCodec as PeerCodec>::Error>> {
        let (id, hash, offset, final_chunk, chunk_hash, content) = decode_blob_response(data)?;
        if let Some(mut pending) = self.pending_reads.remove(&id) {
            if hash != pending.hash
                || offset != pending.offset
                || ContentHash::of(&content) != chunk_hash
            {
                let _ = pending.sender.send(Err(ReadError::CorruptContent));
                return Ok(());
            }
            pending.offset = pending
                .offset
                .checked_add(u64::try_from(content.len()).map_err(|_| SyncError::InvalidMessage)?)
                .ok_or(SyncError::InvalidMessage)?;
            pending.content.extend_from_slice(&content);
            if final_chunk {
                let result = if pending.offset == pending.size
                    && ContentHash::of(&pending.content) == pending.hash
                {
                    Ok(pending.content)
                } else {
                    Err(ReadError::CorruptContent)
                };
                let _ = pending.sender.send(result);
            } else {
                let data = encode_blob_request(
                    id,
                    &pending.path,
                    pending.hash,
                    pending.offset,
                    READ_CHUNK_SIZE,
                );
                let peer = pending.peer.clone();
                self.pending_reads.insert(id, pending);
                return self.emit(SyncRequest::Send { peer, data });
            }
            return Ok(());
        }
        let Some(mut pending) = self.pending_streams.remove(&id) else {
            return Err(SyncError::InvalidMessage);
        };
        if hash != pending.hash
            || offset != pending.offset
            || ContentHash::of(&content) != chunk_hash
        {
            let _ = pending.sender.send(Err(ReadError::CorruptContent));
            return Ok(());
        }
        pending.offset = pending
            .offset
            .checked_add(u64::try_from(content.len()).map_err(|_| SyncError::InvalidMessage)?)
            .ok_or(SyncError::InvalidMessage)?;
        pending.hasher.update(&content);
        if !content.is_empty() {
            let _ = pending.sender.send(Ok(content));
        }
        if final_chunk {
            if pending.offset != pending.size
                || pending.hasher.finalize().as_bytes() != pending.hash.as_bytes()
            {
                let _ = pending.sender.send(Err(ReadError::CorruptContent));
            }
            return Ok(());
        }
        let data = encode_blob_request(
            id,
            &pending.path,
            pending.hash,
            pending.offset,
            pending.chunk_size,
        );
        let peer = pending.peer.clone();
        self.pending_streams.insert(id, pending);
        self.emit(SyncRequest::Send { peer, data })
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
    let mut data = Vec::with_capacity(6 + folder.len() + message.len());
    data.push(PROTOCOL_VERSION);
    data.push(METADATA_MESSAGE);
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
        .get(2..6)
        .ok_or(SyncError::InvalidMessage)?
        .try_into()
        .map_err(|_| SyncError::InvalidMessage)?;
    let length =
        usize::try_from(u32::from_be_bytes(length)).map_err(|_| SyncError::InvalidMessage)?;
    let end = 6usize
        .checked_add(length)
        .ok_or(SyncError::InvalidMessage)?;
    let folder = core::str::from_utf8(data.get(6..end).ok_or(SyncError::InvalidMessage)?)
        .map_err(|_| SyncError::InvalidMessage)?;
    validate_folder(folder).map_err(|_| SyncError::InvalidMessage)?;
    let message = data.get(end..).ok_or(SyncError::InvalidMessage)?;
    if message.is_empty() {
        return Err(SyncError::InvalidMessage);
    }
    Ok((folder.to_string(), message.to_vec()))
}

fn encode_blob_request(
    id: u64,
    path: &str,
    hash: ContentHash,
    offset: u64,
    chunk_size: usize,
) -> Vec<u8> {
    let path = path.as_bytes();
    let length = u32::try_from(path.len()).expect("path exceeds u32");
    let chunk_size = u32::try_from(chunk_size).expect("chunk size exceeds u32");
    let mut data = Vec::with_capacity(58 + path.len());
    data.extend_from_slice(&[PROTOCOL_VERSION, BLOB_REQUEST]);
    data.extend_from_slice(&id.to_be_bytes());
    data.extend_from_slice(hash.as_bytes());
    data.extend_from_slice(&offset.to_be_bytes());
    data.extend_from_slice(&chunk_size.to_be_bytes());
    data.extend_from_slice(&length.to_be_bytes());
    data.extend_from_slice(path);
    data
}

fn decode_blob_request<StorageError, PeerError>(
    data: &[u8],
) -> Result<BlobRequest, SyncError<StorageError, PeerError>> {
    let id = u64::from_be_bytes(
        data.get(2..10)
            .ok_or(SyncError::InvalidMessage)?
            .try_into()
            .map_err(|_| SyncError::InvalidMessage)?,
    );
    let hash = data
        .get(10..42)
        .ok_or(SyncError::InvalidMessage)?
        .try_into()
        .map_err(|_| SyncError::InvalidMessage)?;
    let offset = u64::from_be_bytes(
        data.get(42..50)
            .ok_or(SyncError::InvalidMessage)?
            .try_into()
            .map_err(|_| SyncError::InvalidMessage)?,
    );
    let chunk_size = u32::from_be_bytes(
        data.get(50..54)
            .ok_or(SyncError::InvalidMessage)?
            .try_into()
            .map_err(|_| SyncError::InvalidMessage)?,
    ) as usize;
    let length = u32::from_be_bytes(
        data.get(54..58)
            .ok_or(SyncError::InvalidMessage)?
            .try_into()
            .map_err(|_| SyncError::InvalidMessage)?,
    ) as usize;
    let path = core::str::from_utf8(data.get(58..58 + length).ok_or(SyncError::InvalidMessage)?)
        .map_err(|_| SyncError::InvalidMessage)?;
    split_path(path).map_err(|_| SyncError::InvalidMessage)?;
    Ok((
        id,
        path.to_string(),
        ContentHash::from_bytes(hash),
        offset,
        chunk_size,
    ))
}

fn encode_blob_response(
    id: u64,
    hash: ContentHash,
    offset: u64,
    final_chunk: bool,
    content: Vec<u8>,
) -> Vec<u8> {
    let mut data = Vec::with_capacity(83 + content.len());
    data.extend_from_slice(&[PROTOCOL_VERSION, BLOB_RESPONSE]);
    data.extend_from_slice(&id.to_be_bytes());
    data.extend_from_slice(hash.as_bytes());
    data.extend_from_slice(&offset.to_be_bytes());
    data.push(u8::from(final_chunk));
    data.extend_from_slice(ContentHash::of(&content).as_bytes());
    data.extend_from_slice(&content);
    data
}

fn decode_blob_response<StorageError, PeerError>(
    data: &[u8],
) -> Result<BlobResponse, SyncError<StorageError, PeerError>> {
    let id = u64::from_be_bytes(
        data.get(2..10)
            .ok_or(SyncError::InvalidMessage)?
            .try_into()
            .map_err(|_| SyncError::InvalidMessage)?,
    );
    let hash = data
        .get(10..42)
        .ok_or(SyncError::InvalidMessage)?
        .try_into()
        .map_err(|_| SyncError::InvalidMessage)?;
    let offset = u64::from_be_bytes(
        data.get(42..50)
            .ok_or(SyncError::InvalidMessage)?
            .try_into()
            .map_err(|_| SyncError::InvalidMessage)?,
    );
    let final_chunk = match data.get(50) {
        Some(0) => false,
        Some(1) => true,
        _ => return Err(SyncError::InvalidMessage),
    };
    let chunk_hash = data
        .get(51..83)
        .ok_or(SyncError::InvalidMessage)?
        .try_into()
        .map_err(|_| SyncError::InvalidMessage)?;
    let content = data.get(83..).ok_or(SyncError::InvalidMessage)?.to_vec();
    Ok((
        id,
        ContentHash::from_bytes(hash),
        offset,
        final_chunk,
        ContentHash::from_bytes(chunk_hash),
        content,
    ))
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
