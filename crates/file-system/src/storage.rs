use alloc::{collections::BTreeMap, vec::Vec};

use crate::ContentHash;

#[derive(Debug, Default)]
pub(crate) struct BlobStore {
    blobs: BTreeMap<ContentHash, Vec<u8>>,
}

impl BlobStore {
    pub(crate) fn insert(&mut self, content: &[u8]) -> ContentHash {
        let hash = ContentHash::of(content);
        self.blobs.entry(hash).or_insert_with(|| content.to_vec());
        hash
    }

    pub(crate) fn get(&self, hash: ContentHash) -> Option<&[u8]> {
        self.blobs.get(&hash).map(Vec::as_slice)
    }
}
