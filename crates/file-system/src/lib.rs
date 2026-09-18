#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(feature = "sync")]
mod content_store;

mod backend;
mod error;
#[cfg(feature = "sync")]
mod file_system;
mod fuse;
mod hash;
#[cfg(feature = "sync")]
mod local;
#[cfg(feature = "sync")]
mod local_state;
#[cfg(feature = "in-memory")]
mod memory_storage;
#[cfg(feature = "in-memory")]
mod memory_transport;
mod metadata;
#[cfg(feature = "native")]
mod native_storage;
mod peer;
mod storage;
mod stream;
#[cfg(feature = "sync")]
mod sync_session;
#[cfg(feature = "sync")]
mod sync_store;
mod transport;

pub use error::Error;
#[cfg(feature = "sync")]
pub use file_system::{
    FileSystem, FileSystemError, FileSystemInitError, ReadError, ReadFuture, ReadStream, SyncError,
};
pub use hash::ContentHash;
#[cfg(feature = "sync")]
pub use local::{LocalFileSystem, LocalIncoming, LocalPeer, LocalPeerCodec, LocalTransport};
#[cfg(feature = "sync")]
pub use local_state::{
    ContentRecoveryReport, LocalFileSystemState, MetadataRecoveryReport, OutboundRecoveryReport,
};
#[cfg(feature = "in-memory")]
pub use memory_storage::InMemoryStorage;
#[cfg(feature = "in-memory")]
pub use memory_transport::{MemoryIncoming, MemoryTransport, MemoryTransportMutator};
pub use metadata::{FileEntry, MergeStrategy};
#[cfg(feature = "native")]
pub use native_storage::NativeStorage;
pub use peer::PeerCodec;
pub use storage::Storage;
pub use stream::ChunkStream;
pub use transport::Transport;
