use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
};

use automerge::AutoCommit;
use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};

use crate::Storage;

const DOCUMENTS_DIRECTORY: &str = ".sync/documents";
const DIRTY_DIRECTORY: &str = ".sync/dirty";
const ROOT_FOLDER_KEY: &str = "@root";

#[derive(Debug)]
pub(crate) enum SyncStoreError<E> {
    Storage(E),
    Metadata(automerge::AutomergeError),
}

impl<E: core::fmt::Display> core::fmt::Display for SyncStoreError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Storage(error) => write!(f, "Storage error: {error}"),
            Self::Metadata(error) => write!(f, "Metadata error: {error}"),
        }
    }
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
            let Some(key) = path.strip_prefix(&documents_prefix) else {
                continue;
            };
            let Some(folder) = decode_folder_key(key) else {
                continue;
            };
            let bytes = self
                .storage
                .read(&path)
                .await
                .map_err(SyncStoreError::Storage)?;
            let document = AutoCommit::load(&bytes).map_err(SyncStoreError::Metadata)?;
            documents.insert(folder, document);
        }
        let dirty_prefix = format!("{DIRTY_DIRECTORY}/");
        let dirty = self
            .storage
            .list(DIRTY_DIRECTORY)
            .await
            .map_err(SyncStoreError::Storage)?
            .into_iter()
            .filter_map(|path| path.strip_prefix(&dirty_prefix).and_then(decode_folder_key))
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
            .await?;
        let _ = self.clear_dirty(folder).await;
        Ok(())
    }

    pub(crate) async fn mark_dirty(&mut self, folder: &str) -> Result<(), S::Error> {
        self.storage.write(&dirty_path(folder), &[]).await
    }

    pub(crate) async fn clear_dirty(&mut self, folder: &str) -> Result<(), S::Error> {
        let _ = self.storage.remove(&dirty_path(folder)).await;
        Ok(())
    }

    pub(crate) async fn list_dirty(&self) -> Result<BTreeSet<String>, S::Error> {
        let dirty_prefix = format!("{DIRTY_DIRECTORY}/");
        let dirty = self
            .storage
            .list(DIRTY_DIRECTORY)
            .await?
            .into_iter()
            .filter_map(|path| path.strip_prefix(&dirty_prefix).and_then(decode_folder_key))
            .collect();
        Ok(dirty)
    }
}

fn document_path(folder: &str) -> String {
    format!("{DOCUMENTS_DIRECTORY}/{}", encode_folder_key(folder))
}

fn dirty_path(folder: &str) -> String {
    format!("{DIRTY_DIRECTORY}/{}", encode_folder_key(folder))
}

fn encode_folder_key(folder: &str) -> String {
    if folder.is_empty() {
        return ROOT_FOLDER_KEY.to_string();
    }
    utf8_percent_encode(folder, NON_ALPHANUMERIC).to_string()
}

fn decode_folder_key(key: &str) -> Option<String> {
    if key == ROOT_FOLDER_KEY {
        return Some(String::new());
    }
    String::from_utf8(percent_decode_str(key).collect()).ok()
}
