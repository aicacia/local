use alloc::{collections::BTreeSet, string::String};

use crate::ContentHash;

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
pub struct FileEntry<P> {
    pub name: String,
    pub hash: ContentHash,
    pub size: u64,
    pub providers: BTreeSet<P>,
    pub local: bool,
    pub tombstoned: bool,
    pub merge_strategy: MergeStrategy,
}

impl<P> FileEntry<P> {
    #[must_use]
    pub fn new(
        name: String,
        hash: ContentHash,
        size: u64,
        providers: BTreeSet<P>,
        local: bool,
    ) -> Self {
        let merge_strategy = MergeStrategy::for_name(&name);
        Self {
            name,
            hash,
            size,
            providers,
            local,
            tombstoned: false,
            merge_strategy,
        }
    }
}
