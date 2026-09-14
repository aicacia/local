use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec::Vec,
};

use automerge::AutoCommit;

use crate::{
    ContentHash, FileEntry, FileSystemError, PeerCodec, Storage,
    content_store::ContentStore,
    local::LocalPeerCodec,
    metadata::{
        MetadataError, join_path, load_entries, load_entry, split_path, store_entry,
        store_tombstone, validate_folder,
    },
    sync_store::{SyncStore, SyncStoreError},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MetadataRecoveryReport {
    pub loaded_documents: usize,
    pub dirty_folders: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContentRecoveryReport {
    pub referenced_blobs: usize,
    pub pruned_orphans: usize,
    pub missing_blobs: usize,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OutboundRecoveryReport {
    pub dirty_folders: Vec<String>,
}

pub struct LocalFileSystemState<S: Storage, C: PeerCodec = LocalPeerCodec> {
    pub(crate) content_store: ContentStore<S>,
    pub(crate) local_peer: C::PeerId,
    pub(crate) documents: BTreeMap<String, AutoCommit>,
    pub(crate) dirty_folders: BTreeSet<String>,
}

impl<S: Storage, C: PeerCodec> LocalFileSystemState<S, C> {
    pub async fn open(
        mut storage: S,
        local_peer: C::PeerId,
    ) -> Result<Self, FileSystemError<S::Error>> {
        let sync_state =
            SyncStore::new(&mut storage)
                .load()
                .await
                .map_err(|error| match error {
                    SyncStoreError::Storage(storage) => FileSystemError::Storage(storage),
                    SyncStoreError::Metadata(metadata) => FileSystemError::Metadata(metadata),
                })?;
        Ok(Self {
            content_store: ContentStore::new(storage),
            local_peer,
            documents: sync_state.documents,
            dirty_folders: sync_state.dirty_folders,
        })
    }

    pub(crate) fn content_store(&self) -> &ContentStore<S> {
        &self.content_store
    }

    pub(crate) fn content_store_mut(&mut self) -> &mut ContentStore<S> {
        &mut self.content_store
    }

    pub fn storage(&self) -> &S {
        self.content_store.storage()
    }

    pub fn storage_mut(&mut self) -> &mut S {
        self.content_store.storage_mut()
    }

    pub fn documents(&self) -> &BTreeMap<String, AutoCommit> {
        &self.documents
    }

    pub fn documents_mut(&mut self) -> &mut BTreeMap<String, AutoCommit> {
        &mut self.documents
    }

    pub fn dirty_folders(&self) -> &BTreeSet<String> {
        &self.dirty_folders
    }

    pub fn dirty_folders_mut(&mut self) -> &mut BTreeSet<String> {
        &mut self.dirty_folders
    }

    pub fn local_peer(&self) -> &C::PeerId {
        &self.local_peer
    }

    pub async fn read(&self, path: &str) -> Result<Vec<u8>, FileSystemError<S::Error>> {
        split_path(path).map_err(|_| FileSystemError::InvalidPath)?;
        self.content_store
            .read(path)
            .await
            .map_err(FileSystemError::Storage)
    }

    pub async fn write(
        &mut self,
        path: &str,
        content: &[u8],
    ) -> Result<(FileEntry<C::PeerId>, String), FileSystemError<S::Error>> {
        let (folder, name) = split_path(path).map_err(|_| FileSystemError::InvalidPath)?;
        let hash = self
            .content_store
            .write(path, content)
            .await
            .map_err(FileSystemError::Storage)?;
        let mut providers = BTreeSet::new();
        providers.insert(self.local_peer.clone());
        let entry = FileEntry::new(
            name.to_string(),
            hash,
            u64::try_from(content.len()).expect("content length exceeds u64"),
            providers,
            true,
        );
        let document = self.documents.entry(folder.to_string()).or_default();
        store_entry::<C>(document, &entry).map_err(FileSystemError::Metadata)?;
        self.dirty_folders.insert(folder.to_string());
        self.persist_dirty(folder)
            .await
            .map_err(FileSystemError::Storage)?;
        let _ = self.garbage_collect().await;
        Ok((entry, folder.to_string()))
    }

    pub async fn append(
        &mut self,
        path: &str,
        content: &[u8],
    ) -> Result<(FileEntry<C::PeerId>, String), FileSystemError<S::Error>> {
        let (folder, name) = split_path(path).map_err(|_| FileSystemError::InvalidPath)?;
        let prev_size = self.entry(path).map(|e| e.size).unwrap_or(0);
        let hash = self
            .content_store
            .append(path, content)
            .await
            .map_err(FileSystemError::Storage)?;
        let size = prev_size
            .saturating_add(u64::try_from(content.len()).expect("content length exceeds u64"));
        let mut providers = BTreeSet::new();
        providers.insert(self.local_peer.clone());
        let entry = FileEntry::new(name.to_string(), hash, size, providers, true);
        let document = self.documents.entry(folder.to_string()).or_default();
        store_entry::<C>(document, &entry).map_err(FileSystemError::Metadata)?;
        self.dirty_folders.insert(folder.to_string());
        self.persist_dirty(folder)
            .await
            .map_err(FileSystemError::Storage)?;
        let _ = self.garbage_collect().await;
        Ok((entry, folder.to_string()))
    }

    pub async fn delete(
        &mut self,
        path: &str,
    ) -> Result<Option<String>, FileSystemError<S::Error>> {
        let (folder, name) = split_path(path).map_err(|_| FileSystemError::InvalidPath)?;
        self.content_store
            .delete(path)
            .await
            .map_err(FileSystemError::Storage)?;
        if let Some(document) = self.documents.get_mut(folder) {
            store_tombstone(document, name).map_err(FileSystemError::Metadata)?;
            self.dirty_folders.insert(folder.to_string());
            self.persist_dirty(folder)
                .await
                .map_err(FileSystemError::Storage)?;
            let _ = self.garbage_collect().await;
            Ok(Some(folder.to_string()))
        } else {
            Ok(None)
        }
    }

    pub async fn rename(
        &mut self,
        from: &str,
        to: &str,
    ) -> Result<(Option<String>, String), FileSystemError<S::Error>> {
        let (from_folder, from_name) =
            split_path(from).map_err(|_| FileSystemError::InvalidPath)?;
        let (to_folder, to_name) = split_path(to).map_err(|_| FileSystemError::InvalidPath)?;
        let entry = self.entry(from)?;
        if self.entry(to).is_ok() {
            return Err(FileSystemError::InvalidMetadata);
        }
        self.content_store
            .rename(from, to, entry.local)
            .await
            .map_err(FileSystemError::Storage)?;

        let mut providers = BTreeSet::new();
        providers.insert(self.local_peer.clone());
        let renamed = FileEntry::new(
            to_name.to_string(),
            entry.hash,
            entry.size,
            providers,
            entry.local,
        );

        if let Some(from_document) = self.documents.get_mut(from_folder) {
            store_tombstone(from_document, from_name).map_err(FileSystemError::Metadata)?;
            self.dirty_folders.insert(from_folder.to_string());
            self.persist_dirty(from_folder)
                .await
                .map_err(FileSystemError::Storage)?;
        }

        let to_document = self.documents.entry(to_folder.to_string()).or_default();
        store_entry::<C>(to_document, &renamed).map_err(FileSystemError::Metadata)?;
        self.dirty_folders.insert(to_folder.to_string());
        self.persist_dirty(to_folder)
            .await
            .map_err(FileSystemError::Storage)?;

        let _ = self.garbage_collect().await;
        Ok((Some(from_folder.to_string()), to_folder.to_string()))
    }

    pub fn entry(&self, path: &str) -> Result<FileEntry<C::PeerId>, FileSystemError<S::Error>> {
        let (folder, name) = split_path(path).map_err(|_| FileSystemError::InvalidPath)?;
        let document = self
            .documents
            .get(folder)
            .ok_or(FileSystemError::NotFound)?;
        let mut entry = load_entry::<C>(document, name).map_err(|error| match error {
            MetadataError::Metadata(metadata) => FileSystemError::Metadata(metadata),
            MetadataError::Peer(_) | MetadataError::InvalidMessage => {
                FileSystemError::InvalidMetadata
            }
        })?;
        if entry.tombstoned {
            return Err(FileSystemError::NotFound);
        }
        entry.local = entry.providers.contains(&self.local_peer);
        Ok(entry)
    }

    pub fn list(
        &self,
        folder: &str,
    ) -> Result<Vec<FileEntry<C::PeerId>>, FileSystemError<S::Error>> {
        validate_folder(folder).map_err(|_| FileSystemError::InvalidPath)?;
        let folder = folder.trim_end_matches('/');
        let Some(document) = self.documents.get(folder) else {
            return Ok(Vec::new());
        };
        let entries = load_entries::<C>(document).map_err(|error| match error {
            MetadataError::Metadata(metadata) => FileSystemError::Metadata(metadata),
            MetadataError::Peer(_) | MetadataError::InvalidMessage => {
                FileSystemError::InvalidMetadata
            }
        })?;
        Ok(entries
            .into_values()
            .filter(|entry| !entry.tombstoned)
            .map(|mut entry| {
                entry.local = entry.providers.contains(&self.local_peer);
                entry
            })
            .collect())
    }

    pub fn paths(&self) -> Result<Vec<String>, FileSystemError<S::Error>> {
        let mut paths = Vec::new();
        for (folder, document) in &self.documents {
            let entries = load_entries::<C>(document).map_err(|error| match error {
                MetadataError::Metadata(m) => FileSystemError::Metadata(m),
                MetadataError::Peer(_) | MetadataError::InvalidMessage => {
                    FileSystemError::InvalidMetadata
                }
            })?;
            for entry in entries.values() {
                if !entry.tombstoned {
                    paths.push(join_path(folder, &entry.name));
                }
            }
        }
        Ok(paths)
    }

    pub async fn persist_dirty(&mut self, folder: &str) -> Result<(), S::Error> {
        if self.dirty_folders.contains(folder) {
            if let Some(document) = self.documents.get_mut(folder) {
                SyncStore::new(self.content_store.storage_mut())
                    .persist(folder, document)
                    .await?;
                self.dirty_folders.remove(folder);
            }
        }
        Ok(())
    }

    pub async fn garbage_collect(&mut self) -> Result<usize, S::Error> {
        let mut referenced = BTreeSet::new();
        for document in self.documents.values() {
            if let Ok(entries) = load_entries::<C>(document) {
                for entry in entries.values() {
                    if !entry.tombstoned {
                        referenced.insert(entry.hash);
                    }
                }
            }
        }
        self.content_store.remove_unreferenced(&referenced).await?;
        Ok(0)
    }

    pub async fn recover_metadata(
        &mut self,
    ) -> Result<MetadataRecoveryReport, FileSystemError<S::Error>> {
        let sync_state = SyncStore::new(self.content_store.storage_mut())
            .load()
            .await
            .map_err(|error| match error {
                SyncStoreError::Storage(storage) => FileSystemError::Storage(storage),
                SyncStoreError::Metadata(metadata) => FileSystemError::Metadata(metadata),
            })?;
        self.documents = sync_state.documents;
        self.dirty_folders = sync_state.dirty_folders;
        Ok(MetadataRecoveryReport {
            loaded_documents: self.documents.len(),
            dirty_folders: self.dirty_folders.iter().cloned().collect(),
        })
    }

    pub async fn recover_content(
        &mut self,
    ) -> Result<ContentRecoveryReport, FileSystemError<S::Error>> {
        let mut referenced = BTreeSet::new();
        for document in self.documents.values() {
            if let Ok(entries) = load_entries::<C>(document) {
                for entry in entries.values() {
                    if !entry.tombstoned {
                        referenced.insert(entry.hash);
                    }
                }
            }
        }
        let blobs_prefix = ".blobs/";
        let blob_paths = self
            .content_store
            .storage()
            .list(".blobs")
            .await
            .map_err(FileSystemError::Storage)?;
        let mut stored_blobs = BTreeSet::new();
        let mut pruned_orphans = 0;
        for path in blob_paths {
            if let Some(hash_str) = path.strip_prefix(blobs_prefix) {
                if let Ok(hash) = ContentHash::from_hex(hash_str) {
                    stored_blobs.insert(hash);
                    if !referenced.contains(&hash) {
                        let _ = self.content_store.storage_mut().remove(&path).await;
                        pruned_orphans += 1;
                    }
                }
            }
        }
        let mut missing_blobs = 0;
        for hash in &referenced {
            if !stored_blobs.contains(hash) {
                missing_blobs += 1;
            }
        }
        Ok(ContentRecoveryReport {
            referenced_blobs: referenced.len(),
            pruned_orphans,
            missing_blobs,
        })
    }

    pub async fn recover_outbound(
        &mut self,
    ) -> Result<OutboundRecoveryReport, FileSystemError<S::Error>> {
        let dirty = SyncStore::new(self.content_store.storage_mut())
            .list_dirty()
            .await
            .map_err(FileSystemError::Storage)?;
        self.dirty_folders.extend(dirty);
        Ok(OutboundRecoveryReport {
            dirty_folders: self.dirty_folders.iter().cloned().collect(),
        })
    }

    pub async fn flush_outbound(&mut self) -> Result<(), FileSystemError<S::Error>> {
        let folders: Vec<String> = self.dirty_folders.iter().cloned().collect();
        for folder in folders {
            if let Some(document) = self.documents.get_mut(&folder) {
                SyncStore::new(self.content_store.storage_mut())
                    .persist(&folder, document)
                    .await
                    .map_err(FileSystemError::Storage)?;
                self.dirty_folders.remove(&folder);
            }
        }
        Ok(())
    }
}
