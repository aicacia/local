use alloc::{collections::BTreeSet, vec::Vec};

use crate::{ChunkStream, ContentHash, FileEntry, PeerCodec};

pub trait Storage {
    type Error;
    type PeerId: Ord + Clone;
    type PeerCodec: PeerCodec<PeerId = Self::PeerId>;

    fn write(&mut self, path: &str, content: &[u8])
    -> Result<FileEntry<Self::PeerId>, Self::Error>;
    fn register_passthrough(
        &mut self,
        path: &str,
        hash: ContentHash,
        size: u64,
        providers: BTreeSet<Self::PeerId>,
    ) -> Result<FileEntry<Self::PeerId>, Self::Error>;
    fn entry(&self, path: &str) -> Result<FileEntry<Self::PeerId>, Self::Error>;
    fn list(&self, folder: &str) -> Result<Vec<FileEntry<Self::PeerId>>, Self::Error>;
    fn read(&self, path: &str) -> Result<Vec<u8>, Self::Error>;
    fn stream(&self, path: &str, chunk_size: usize) -> Result<ChunkStream, Self::Error>;
}
