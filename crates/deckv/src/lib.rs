#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(feature = "in-memory")]
mod in_memory;
mod lww_record;
mod storage;
mod store;
mod sync;

#[cfg(feature = "in-memory")]
pub use in_memory::InMemoryStorage;
pub use lww_record::LwwRecord;
pub use storage::Storage;
pub use store::Store;
pub use sync::{Sync, SyncMessage};
