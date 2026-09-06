use alloc::{collections::BTreeSet, string::String};

use crate::{ContentHash, PeerId};

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
pub struct FileEntry {
    pub name: String,
    pub hash: ContentHash,
    pub size: u64,
    pub providers: BTreeSet<PeerId>,
    pub local: bool,
    pub merge_strategy: MergeStrategy,
}

impl FileEntry {
    #[must_use]
    pub fn new(
        name: String,
        hash: ContentHash,
        size: u64,
        providers: BTreeSet<PeerId>,
        local: bool,
    ) -> Self {
        let merge_strategy = MergeStrategy::for_name(&name);
        Self {
            name,
            hash,
            size,
            providers,
            local,
            merge_strategy,
        }
    }
}
