use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use file_system::{ContentHash, FileSystem, NativeStorage, PeerCodec, Transport};
use idp_model::contract::{
    GLOBAL_IDENTITY_MANIFEST_VERSION, GlobalIdentityManifest, GlobalIdentityRecord,
    GlobalIdentityRow, GlobalIdentityTable, GlobalIdentityValue,
};
use iroh_chain::VaultId;

pub type GlobalIdentityFileSystem<C, T> = FileSystem<NativeStorage, C, T>;

pub struct GlobalIdentityRuntime<C, T>
where
    C: PeerCodec,
    T: Transport<PeerId = C::PeerId>,
{
    file_system: Arc<GlobalIdentityFileSystem<C, T>>,
    root: PathBuf,
}

impl<C, T> GlobalIdentityRuntime<C, T>
where
    C: PeerCodec + Send + Sync + 'static,
    C::Error: std::fmt::Display + Send + 'static,
    C::PeerId: Send + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Error: std::fmt::Display,
    T::Incoming: Send + 'static,
{
    pub async fn new(root: PathBuf, local_peer: C::PeerId, transport: T) -> Result<Self, String> {
        let storage =
            NativeStorage::new(root.join("global-identity")).map_err(|error| error.to_string())?;
        let file_system = FileSystem::new(storage, local_peer, transport)
            .await
            .map_err(|error| error.to_string())?;
        Ok(Self {
            file_system: Arc::new(file_system),
            root: root.join("global-identity"),
        })
    }

    #[must_use]
    pub fn file_system(&self) -> Arc<GlobalIdentityFileSystem<C, T>> {
        Arc::clone(&self.file_system)
    }

    #[must_use]
    pub fn vault_id(&self) -> VaultId {
        VaultId::global_identity()
    }

    pub async fn stage_revision(
        &self,
        revision: String,
        mut rows: Vec<GlobalIdentityRow>,
    ) -> Result<GlobalIdentityManifest, String> {
        if !valid_revision(&revision) || rows.is_empty() {
            return Err("invalid global identity revision".to_owned());
        }

        rows.sort_by_key(GlobalIdentityRow::path);
        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            if !row.is_valid() {
                return Err("invalid global identity row".to_owned());
            }
            let path = row.path();
            if records
                .last()
                .is_some_and(|record: &GlobalIdentityRecord| record.path == path)
            {
                return Err("duplicate global identity row".to_owned());
            }
            let content = serde_json::to_vec(&row).map_err(|error| error.to_string())?;
            let hash = ContentHash::of(&content).to_string();
            self.file_system
                .write(&revision_path(&revision, &path), &content)
                .await
                .map_err(|error| error.to_string())?;
            records.push(GlobalIdentityRecord { path, hash });
        }

        let manifest = GlobalIdentityManifest {
            version: GLOBAL_IDENTITY_MANIFEST_VERSION,
            revision: revision.clone(),
            records,
        };
        if !manifest.is_valid() {
            return Err("invalid global identity manifest".to_owned());
        }
        let content = serde_json::to_vec(&manifest).map_err(|error| error.to_string())?;
        self.file_system
            .write(&manifest_path(&revision), &content)
            .await
            .map_err(|error| error.to_string())?;
        Ok(manifest)
    }

    pub async fn synchronize(&self, peer: C::PeerId) -> Result<(), String> {
        self.file_system
            .sync_peer(peer)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn activate_revision(
        &self,
        revision: &str,
    ) -> Result<GlobalIdentityManifest, String> {
        let (manifest, _) = self.read_revision(revision).await?;
        write_active_manifest(&self.root, &manifest)?;
        Ok(manifest)
    }

    pub fn active_manifest(&self) -> Result<Option<GlobalIdentityManifest>, String> {
        let path = active_manifest_path(&self.root);
        if !path.exists() {
            return Ok(None);
        }
        let manifest = serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        Ok(Some(manifest))
    }

    pub async fn active_rows(
        &self,
    ) -> Result<Option<(GlobalIdentityManifest, Vec<GlobalIdentityRow>)>, String> {
        let Some(active_manifest) = self.active_manifest()? else {
            return Ok(None);
        };
        let (manifest, rows) = self.read_revision(&active_manifest.revision).await?;
        if manifest != active_manifest {
            return Err("active global identity manifest does not match revision".to_owned());
        }
        Ok(Some((manifest, rows)))
    }

    pub async fn has_approved_device(&self, public_key: &str) -> Result<bool, String> {
        let Some((_, rows)) = self.active_rows().await? else {
            return Ok(false);
        };
        Ok(rows.into_iter().any(|row| {
            row.table == GlobalIdentityTable::Devices
                && row.columns.get("public_key")
                    == Some(&GlobalIdentityValue::Text(public_key.to_owned()))
                && row.columns.get("state") == Some(&GlobalIdentityValue::Integer(1))
                && row.columns.get("revoked_at") == Some(&GlobalIdentityValue::Null)
        }))
    }

    async fn read_revision(
        &self,
        revision: &str,
    ) -> Result<(GlobalIdentityManifest, Vec<GlobalIdentityRow>), String> {
        if !valid_revision(revision) {
            return Err("invalid global identity revision".to_owned());
        }
        let manifest: GlobalIdentityManifest = serde_json::from_slice(
            &self
                .file_system
                .read(&manifest_path(revision))
                .await
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        if manifest.revision != revision || !manifest.is_valid() {
            return Err("invalid global identity manifest".to_owned());
        }
        let mut rows = Vec::with_capacity(manifest.records.len());
        for record in &manifest.records {
            let content = self
                .file_system
                .read(&revision_path(revision, &record.path))
                .await
                .map_err(|error| error.to_string())?;
            if ContentHash::of(&content).to_string() != record.hash {
                return Err("global identity record hash mismatch".to_owned());
            }
            let row: GlobalIdentityRow =
                serde_json::from_slice(&content).map_err(|error| error.to_string())?;
            if !row.is_valid() || row.path() != record.path {
                return Err("invalid global identity record".to_owned());
            }
            rows.push(row);
        }
        Ok((manifest, rows))
    }
}

fn revision_path(revision: &str, path: &str) -> String {
    format!("revisions/{revision}/{path}")
}

fn manifest_path(revision: &str) -> String {
    format!("revisions/{revision}/manifest.json")
}

fn active_manifest_path(root: &Path) -> PathBuf {
    root.join("active-revision.json")
}

fn valid_revision(revision: &str) -> bool {
    !revision.is_empty() && !revision.contains('/') && revision != "." && revision != ".."
}

fn write_active_manifest(root: &Path, manifest: &GlobalIdentityManifest) -> Result<(), String> {
    let path = active_manifest_path(root);
    let temporary = root.join("active-revision.json.tmp");
    let content = serde_json::to_vec(manifest).map_err(|error| error.to_string())?;
    fs::write(&temporary, content).map_err(|error| error.to_string())?;
    fs::rename(temporary, path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::{
        convert::Infallible,
        env, fs,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use file_system::{MemoryTransport, PeerCodec};
    use idp_model::contract::{GlobalIdentityRow, GlobalIdentityTable, GlobalIdentityValue};
    use iroh_chain::VaultId;

    use super::GlobalIdentityRuntime;

    #[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
    struct Peer;

    impl PeerCodec for Peer {
        type Error = Infallible;
        type PeerId = Self;

        fn encode(_: &Self::PeerId) -> Vec<u8> {
            Vec::new()
        }

        fn decode(_: &[u8]) -> Result<Self::PeerId, Self::Error> {
            Ok(Self)
        }
    }

    fn application_row() -> GlobalIdentityRow {
        let mut columns = std::collections::BTreeMap::new();
        columns.insert(
            "name".to_owned(),
            GlobalIdentityValue::Text("app".to_owned()),
        );
        columns.insert(
            "uri".to_owned(),
            GlobalIdentityValue::Text("https://app".to_owned()),
        );
        columns.insert("description".to_owned(), GlobalIdentityValue::Null);
        columns.insert("created_at".to_owned(), GlobalIdentityValue::Integer(1));
        columns.insert("updated_at".to_owned(), GlobalIdentityValue::Integer(1));
        GlobalIdentityRow {
            table: GlobalIdentityTable::Applications,
            id: 1,
            columns,
        }
    }

    fn device_row(
        public_key: &str,
        state: i64,
        revoked_at: GlobalIdentityValue,
    ) -> GlobalIdentityRow {
        let mut columns = std::collections::BTreeMap::new();
        columns.insert(
            "name".to_owned(),
            GlobalIdentityValue::Text("device".to_owned()),
        );
        columns.insert(
            "public_key".to_owned(),
            GlobalIdentityValue::Text(public_key.to_owned()),
        );
        columns.insert(
            "address".to_owned(),
            GlobalIdentityValue::Text("address".to_owned()),
        );
        columns.insert("enrollment_code_hash".to_owned(), GlobalIdentityValue::Null);
        columns.insert(
            "enrollment_expires_at".to_owned(),
            GlobalIdentityValue::Null,
        );
        columns.insert(
            "pairing_accepting_public_key".to_owned(),
            GlobalIdentityValue::Null,
        );
        columns.insert("state".to_owned(), GlobalIdentityValue::Integer(state));
        columns.insert("created_at".to_owned(), GlobalIdentityValue::Integer(1));
        columns.insert("updated_at".to_owned(), GlobalIdentityValue::Integer(1));
        columns.insert("revoked_at".to_owned(), revoked_at);
        GlobalIdentityRow {
            table: GlobalIdentityTable::Devices,
            id: 1,
            columns,
        }
    }

    #[tokio::test]
    async fn uses_the_global_identity_vault_and_root() {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let root = env::temp_dir().join(format!(
            "global-identity-runtime-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        let (transport, _) = MemoryTransport::pair(Peer, Peer);

        let runtime = GlobalIdentityRuntime::<Peer, _>::new(root.clone(), Peer, transport)
            .await
            .unwrap();

        assert_eq!(runtime.vault_id(), VaultId::global_identity());
        runtime
            .file_system()
            .write("identity.json", b"{}")
            .await
            .unwrap();
        assert!(root.join("global-identity").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn activates_only_complete_verified_revisions() {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let root = env::temp_dir().join(format!(
            "global-identity-activation-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        let (transport, _) = MemoryTransport::pair(Peer, Peer);
        let runtime = GlobalIdentityRuntime::<Peer, _>::new(root.clone(), Peer, transport)
            .await
            .unwrap();

        let manifest = runtime
            .stage_revision("revision-1".to_owned(), vec![application_row()])
            .await
            .unwrap();
        assert_eq!(runtime.active_manifest().unwrap(), None);
        assert_eq!(
            runtime.activate_revision("revision-1").await.unwrap(),
            manifest
        );
        assert_eq!(runtime.active_manifest().unwrap(), Some(manifest.clone()));
        assert_eq!(
            runtime.active_rows().await.unwrap(),
            Some((manifest, vec![application_row()]))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn verifies_only_approved_unrevoked_devices_from_the_active_revision() {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let root = env::temp_dir().join(format!(
            "global-identity-device-approval-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        let (transport, _) = MemoryTransport::pair(Peer, Peer);
        let runtime = GlobalIdentityRuntime::<Peer, _>::new(root.clone(), Peer, transport)
            .await
            .unwrap();

        runtime
            .stage_revision(
                "revision-1".to_owned(),
                vec![device_row("approved", 1, GlobalIdentityValue::Null)],
            )
            .await
            .unwrap();
        runtime.activate_revision("revision-1").await.unwrap();

        assert!(runtime.has_approved_device("approved").await.unwrap());
        assert!(!runtime.has_approved_device("missing").await.unwrap());

        let _ = fs::remove_dir_all(root);
    }
}
