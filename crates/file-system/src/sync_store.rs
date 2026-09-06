use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
};

use automerge::AutoCommit;

use crate::Storage;

const DOCUMENTS_DIRECTORY: &str = ".sync/documents";
const DIRTY_DIRECTORY: &str = ".sync/dirty";

pub(crate) enum SyncStoreError<E> {
    Storage(E),
    Metadata(automerge::AutomergeError),
}

pub(crate) struct SyncState {
    pub(crate) documents: BTreeMap<String, AutoCommit>,
    pub(crate) dirty_folders: BTreeSet<String>,
}

pub(crate) struct SyncStore<'a, S> {
    storage: &'a mut S,
}

impl<'a, S: Storage> SyncStore<'a, S> {
    pub(crate) fn new(storage: &'a mut S) -> Self {
        Self { storage }
    }

    pub(crate) async fn load(&self) -> Result<SyncState, SyncStoreError<S::Error>> {
        let mut documents = BTreeMap::new();
        let documents_prefix = format!("{DOCUMENTS_DIRECTORY}/");
        for path in self
            .storage
            .list(DOCUMENTS_DIRECTORY)
            .await
            .map_err(SyncStoreError::Storage)?
        {
            let Some(folder) = path.strip_prefix(&documents_prefix) else {
                continue;
            };
            let bytes = self
                .storage
                .read(&path)
                .await
                .map_err(SyncStoreError::Storage)?;
            let document = AutoCommit::load(&bytes).map_err(SyncStoreError::Metadata)?;
            documents.insert(folder.to_string(), document);
        }
        let dirty_prefix = format!("{DIRTY_DIRECTORY}/");
        let dirty = self
            .storage
            .list(DIRTY_DIRECTORY)
            .await
            .map_err(SyncStoreError::Storage)?
            .into_iter()
            .filter_map(|path| path.strip_prefix(&dirty_prefix).map(ToString::to_string))
            .collect();
        Ok(SyncState {
            documents,
            dirty_folders: dirty,
        })
    }

    pub(crate) async fn persist(
        &mut self,
        folder: &str,
        document: &mut AutoCommit,
    ) -> Result<(), S::Error> {
        self.storage
            .write(&document_path(folder), &document.save())
            .await
    }

    pub(crate) async fn mark_dirty(&mut self, folder: &str) -> Result<(), S::Error> {
        self.storage.write(&dirty_path(folder), &[]).await
    }
}

fn document_path(folder: &str) -> String {
    format!("{DOCUMENTS_DIRECTORY}/{folder}")
}

fn dirty_path(folder: &str) -> String {
    format!("{DIRTY_DIRECTORY}/{folder}")
}
