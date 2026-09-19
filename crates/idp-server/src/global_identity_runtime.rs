use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use file_system::{FileSystem, Residency, Transport};
use idp_model::contract::{
    GLOBAL_IDENTITY_MANIFEST_VERSION, GlobalIdentityManifest, GlobalIdentityRecord,
    GlobalIdentityRow, GlobalIdentityTable, GlobalIdentityValue,
};
use iroh::EndpointId;
use iroh_chain::VaultId;

pub struct GlobalIdentityRuntime<T>
where
    T: Transport<EndpointId>,
{
    file_system: Arc<FileSystem<EndpointId>>,
    root: PathBuf,
    transport: T,
}

impl<T> GlobalIdentityRuntime<T>
where
    T: Transport<EndpointId> + Clone + Send + Sync + 'static,
    T::Error: std::fmt::Display,
{
    pub async fn new(root: PathBuf, local_peer: EndpointId, transport: T) -> Result<Self, String> {
        let root = root.join("global-identity");
        let file_system =
            Arc::new(FileSystem::open(&root, local_peer).map_err(|error| error.to_string())?);
        file_system
            .set_residency("", Residency::Full)
            .await
            .map_err(|error| error.to_string())?;
        let synchronized_file_system = Arc::clone(&file_system);
        let synchronized_transport = transport.clone();
        tokio::spawn(async move {
            loop {
                let mut sync =
                    synchronized_file_system.metadata_sync(synchronized_transport.clone());
                let _ = sync.pump().await;
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });
        Ok(Self {
            file_system,
            root,
            transport,
        })
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
            let entry = self
                .file_system
                .write(&revision_path(&revision, &path), &content)
                .await
                .map_err(|error| error.to_string())?;
            records.push(GlobalIdentityRecord {
                path,
                hash: entry.meta.pointer.unwrap_or_default(),
            });
        }
        let manifest = GlobalIdentityManifest {
            version: GLOBAL_IDENTITY_MANIFEST_VERSION,
            revision: revision.clone(),
            records,
        };
        if !manifest.is_valid() {
            return Err("invalid global identity manifest".to_owned());
        }
        self.file_system
            .write(
                &manifest_path(&revision),
                &serde_json::to_vec(&manifest).map_err(|error| error.to_string())?,
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(manifest)
    }

    pub async fn synchronize(&self, _: EndpointId) -> Result<(), String> {
        let mut sync = self.file_system.metadata_sync(self.transport.clone());
        sync.announce().await.map_err(|error| error.to_string())?;
        for _ in 0..32 {
            sync.pump().await.map_err(|error| error.to_string())?;
            tokio::task::yield_now().await;
        }
        Ok(())
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
        serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
            .map(Some)
            .map_err(|error| error.to_string())
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
            let entry = self
                .file_system
                .entry(&revision_path(revision, &record.path))
                .await
                .map_err(|error| error.to_string())?;
            if entry.meta.pointer.as_deref() != Some(&record.hash) {
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
    let temporary = root.join("active-revision.json.tmp");
    fs::write(
        &temporary,
        serde_json::to_vec(manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::rename(temporary, active_manifest_path(root)).map_err(|error| error.to_string())
}
