#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod error;
#[cfg(feature = "in-memory")]
mod file_system;
mod hash;
mod metadata;
#[cfg(feature = "native")]
mod native;
#[cfg(feature = "in-memory")]
mod storage;
mod stream;
mod transport;

pub use error::Error;
#[cfg(feature = "in-memory")]
pub use file_system::InMemoryFileSystem;
#[cfg(feature = "native")]
pub use native::NativeFileSystem;
pub use stream::ChunkStream;
#[cfg(feature = "native")]
pub type FileSystem = NativeFileSystem;
#[cfg(all(not(feature = "native"), feature = "in-memory"))]
pub type FileSystem = InMemoryFileSystem;
pub use hash::ContentHash;
pub use metadata::{FileEntry, MergeStrategy};
pub use transport::{PeerId, Transport};
