use alloc::vec::Vec;

use crate::{ChunkStream, ContentHash};

pub trait Storage {
    type Error;

    fn write(&mut self, path: &str, content: &[u8]) -> Result<ContentHash, Self::Error>;
    fn append(&mut self, path: &str, content: &[u8]) -> Result<ContentHash, Self::Error>;
    fn register_passthrough(&mut self, path: &str) -> Result<(), Self::Error>;
    fn read_file(&self, path: &str) -> Result<Vec<u8>, Self::Error>;
    fn stream(&self, path: &str, chunk_size: usize) -> Result<ChunkStream, Self::Error>;
}
