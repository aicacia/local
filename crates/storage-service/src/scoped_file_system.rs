use std::{collections::BTreeMap, fmt::Debug, io, path::PathBuf, sync::Arc};

use file_system::{FileSystem, Residency};

use crate::AuthorizationStore;
use serde::{Serialize, de::DeserializeOwned};
use storage_model::StorageNamespace;
use tokio::sync::Mutex;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct StorageNamespaceId {
    user_sub: String,
    application_id: idp_model::model::Id,
}

pub type ScopedFileSystem<PeerId> = FileSystem<PeerId>;

pub struct ScopedFileSystemRuntime<PeerId>
where
    PeerId: Clone + Debug + Ord + Serialize + DeserializeOwned + 'static,
{
    root: PathBuf,
    local_peer: PeerId,
    file_systems: Mutex<BTreeMap<StorageNamespaceId, Arc<ScopedFileSystem<PeerId>>>>,
    authorizations: Mutex<BTreeMap<StorageNamespaceId, Arc<AuthorizationStore>>>,
}

impl<PeerId> ScopedFileSystemRuntime<PeerId>
where
    PeerId: Clone + Debug + Ord + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    pub fn new(root: PathBuf, local_peer: PeerId) -> io::Result<Self> {
        std::fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            local_peer,
            file_systems: Mutex::new(BTreeMap::new()),
            authorizations: Mutex::new(BTreeMap::new()),
        })
    }

    pub async fn open<S: StorageNamespace>(
        &self,
        scope: &S,
    ) -> Result<Arc<ScopedFileSystem<PeerId>>, String> {
        let id = storage_namespace_id(scope)?;
        let mut file_systems = self.file_systems.lock().await;
        if let Some(file_system) = file_systems.get(&id) {
            return Ok(Arc::clone(file_system));
        }
        let file_system = Arc::new(
            FileSystem::open(namespace_root(&self.root, &id), self.local_peer.clone())
                .map_err(|error| error.to_string())?,
        );
        file_system
            .set_residency("", Residency::Full)
            .await
            .map_err(|error| error.to_string())?;
        file_systems.insert(id, Arc::clone(&file_system));
        Ok(file_system)
    }

    pub async fn authorization<S: StorageNamespace>(
        &self,
        scope: &S,
    ) -> Result<Arc<AuthorizationStore>, String> {
        let id = storage_namespace_id(scope)?;
        self.open(scope).await?;
        let mut authorizations = self.authorizations.lock().await;
        if let Some(authorization) = authorizations.get(&id) {
            return Ok(Arc::clone(authorization));
        }
        let authorization = Arc::new(
            AuthorizationStore::open(namespace_root(&self.root, &id))
                .map_err(|error| error.to_string())?,
        );
        authorizations.insert(id, Arc::clone(&authorization));
        Ok(authorization)
    }
}

fn namespace_root(root: &std::path::Path, id: &StorageNamespaceId) -> PathBuf {
    root.join("vaults")
        .join(&id.user_sub)
        .join(id.application_id.to_string())
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use storage_model::StorageNamespace;

    use super::ScopedFileSystemRuntime;

    struct Namespace;

    impl StorageNamespace for Namespace {
        fn user_sub(&self) -> &str {
            "user"
        }

        fn application_id(&self) -> idp_model::model::Id {
            1
        }
    }

    #[tokio::test]
    async fn caches_authorization_per_namespace() {
        let root = env::temp_dir().join(format!("storage-runtime-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let runtime = ScopedFileSystemRuntime::new(root.clone(), 1_u8).unwrap();
        let namespace = Namespace;
        let first = runtime.authorization(&namespace).await.unwrap();
        let second = runtime.authorization(&namespace).await.unwrap();
        assert!(std::sync::Arc::ptr_eq(&first, &second));
        let _ = fs::remove_dir_all(root);
    }
}

fn storage_namespace_id(scope: &impl StorageNamespace) -> Result<StorageNamespaceId, String> {
    let user_sub = scope.user_sub();
    if user_sub.is_empty()
        || !user_sub
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err("invalid user subject".to_owned());
    }
    if scope.application_id() <= 0 {
        return Err("invalid application id".to_owned());
    }
    Ok(StorageNamespaceId {
        user_sub: user_sub.to_owned(),
        application_id: scope.application_id(),
    })
}
