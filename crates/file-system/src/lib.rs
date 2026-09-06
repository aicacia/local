#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod error;
mod file_system;
mod hash;
#[cfg(feature = "in-memory")]
mod memory_storage;
mod metadata;
#[cfg(feature = "native")]
mod native_storage;
mod storage;
mod stream;
mod transport;

pub use error::Error;
pub use file_system::FileSystem;
pub use hash::ContentHash;
#[cfg(feature = "in-memory")]
pub use memory_storage::InMemoryStorage;
pub use metadata::{FileEntry, MergeStrategy};
#[cfg(feature = "native")]
pub use native_storage::{NativeStorage, PeerCodec};
pub use storage::Storage;
pub use stream::ChunkStream;
pub use transport::Transport;
