use std::{
    collections::BTreeMap,
    fmt::Debug,
    io,
    path::PathBuf,
    sync::{Arc, Mutex as StdMutex},
};

use file_system::{FileSystem, FileSystemCatalog, FileSystemId, FileSystemResource, Residency};

use serde::{Serialize, de::DeserializeOwned};
use storage_model::StorageNamespace;
use storage_model::{ResourceCatalog, ResourceIdentity, ResourceKind};
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

    catalogs: StdMutex<BTreeMap<StorageNamespaceId, Arc<FileSystemCatalog>>>,
    resources: Mutex<BTreeMap<(StorageNamespaceId, FileSystemId), Arc<ScopedFileSystem<PeerId>>>>,
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

            catalogs: StdMutex::new(BTreeMap::new()),
            resources: Mutex::new(BTreeMap::new()),
        })
    }

    pub async fn create_resource<S: StorageNamespace>(
        &self,
        scope: &S,
        name: Option<String>,
    ) -> Result<FileSystemResource, String> {
        let id = storage_namespace_id(scope)?;
        self.catalog(&id)?
            .create(name)
            .map_err(|error| error.to_string())
    }

    pub async fn list_resources<S: StorageNamespace>(
        &self,
        scope: &S,
    ) -> Result<Vec<FileSystemResource>, String> {
        let id = storage_namespace_id(scope)?;
        self.catalog(&id)?.list().map_err(|error| error.to_string())
    }

    pub async fn delete_resource<S: StorageNamespace>(
        &self,
        scope: &S,
        resource_id: FileSystemId,
    ) -> Result<bool, String> {
        let id = storage_namespace_id(scope)?;
        let mut resources = self.resources.lock().await;
        match self.catalog(&id)?.delete(resource_id) {
            Ok(()) => {
                resources.remove(&(id, resource_id));
                Ok(true)
            }
            Err(file_system::Error::NotFound) => Ok(false),
            Err(error) => Err(error.to_string()),
        }
    }

    pub async fn open_resource<S: StorageNamespace>(
        &self,
        scope: &S,
        resource_id: FileSystemId,
    ) -> Result<Arc<ScopedFileSystem<PeerId>>, String> {
        let id = storage_namespace_id(scope)?;
        let mut resources = self.resources.lock().await;
        if let Some(file_system) = resources.get(&(id.clone(), resource_id)) {
            return Ok(Arc::clone(file_system));
        }
        let file_system = Arc::new(
            self.catalog(&id)?
                .open_filesystem(resource_id, self.local_peer.clone())
                .map_err(|error| error.to_string())?,
        );
        file_system
            .set_residency("", Residency::Full)
            .await
            .map_err(|error| error.to_string())?;
        resources.insert((id, resource_id), Arc::clone(&file_system));
        Ok(file_system)
    }

    fn catalog(&self, id: &StorageNamespaceId) -> Result<Arc<FileSystemCatalog>, String> {
        let mut catalogs = self
            .catalogs
            .lock()
            .expect("filesystem catalog lock poisoned");
        if let Some(catalog) = catalogs.get(id) {
            return Ok(Arc::clone(catalog));
        }
        let catalog = Arc::new(
            FileSystemCatalog::open(namespace_root(&self.root, id))
                .map_err(|error| error.to_string())?,
        );
        catalogs.insert(id.clone(), Arc::clone(&catalog));
        Ok(catalog)
    }
}

impl<PeerId> ResourceCatalog for ScopedFileSystemRuntime<PeerId>
where
    PeerId: Clone + Debug + Ord + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    fn contains(
        &self,
        namespace: &dyn StorageNamespace,
        resource: &ResourceIdentity,
    ) -> Result<bool, String> {
        if resource.kind != ResourceKind::FileSystem {
            return Ok(false);
        }
        let Ok(resource_id) = FileSystemId::parse(&resource.id) else {
            return Ok(false);
        };
        let id = storage_namespace_id(&NamespaceRef(namespace))?;
        self.catalog(&id)?
            .list()
            .map(|resources| resources.iter().any(|entry| entry.id == resource_id))
            .map_err(|error| error.to_string())
    }
}

struct NamespaceRef<'a>(&'a dyn StorageNamespace);

impl StorageNamespace for NamespaceRef<'_> {
    fn user_sub(&self) -> &str {
        self.0.user_sub()
    }

    fn application_id(&self) -> idp_model::model::Id {
        self.0.application_id()
    }
}

fn namespace_root(root: &std::path::Path, id: &StorageNamespaceId) -> PathBuf {
    root.join("filesystems")
        .join(&id.user_sub)
        .join(id.application_id.to_string())
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use storage_model::{ResourceCatalog, ResourceIdentity, ResourceKind, StorageNamespace};

    use super::{ScopedFileSystemRuntime, storage_namespace_id};

    struct Namespace {
        user_sub: &'static str,
        application_id: idp_model::model::Id,
    }

    impl StorageNamespace for Namespace {
        fn user_sub(&self) -> &str {
            self.user_sub
        }

        fn application_id(&self) -> idp_model::model::Id {
            self.application_id
        }
    }

    #[test]
    fn rejects_path_traversal_subjects() {
        for user_sub in [".", ".."] {
            let namespace = Namespace {
                user_sub,
                application_id: idp_model::model::Id::now_v7(),
            };
            assert!(storage_namespace_id(&namespace).is_err());
        }
    }

    #[tokio::test]
    async fn manages_multiple_resources_per_namespace_and_blocks_deleted_opens() {
        let root = env::temp_dir().join(format!("storage-resource-runtime-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let application_id = idp_model::model::Id::now_v7();
        let namespace = Namespace {
            user_sub: "user",
            application_id,
        };
        let runtime = ScopedFileSystemRuntime::new(root.clone(), 1_u8).expect("runtime opens");
        let first = runtime
            .create_resource(&namespace, Some("same".into()))
            .await
            .expect("first resource is created");
        let second = runtime
            .create_resource(&namespace, Some("same".into()))
            .await
            .expect("second resource is created");
        assert_ne!(first.id, second.id);
        let identity = ResourceIdentity {
            kind: ResourceKind::FileSystem,
            id: first.id.as_uuid().to_string(),
        };
        assert!(
            runtime
                .contains(&namespace, &identity)
                .expect("catalog lookup works")
        );

        assert!(
            !runtime
                .contains(
                    &namespace,
                    &ResourceIdentity {
                        kind: ResourceKind::Database,
                        id: identity.id.clone(),
                    },
                )
                .expect("cross-kind lookup works")
        );
        assert_eq!(
            runtime
                .list_resources(&namespace)
                .await
                .expect("resources list"),
            vec![first.clone(), second]
        );

        let first_fs = runtime
            .open_resource(&namespace, first.id)
            .await
            .expect("first filesystem opens");
        let other_namespace = Namespace {
            user_sub: "other",
            application_id,
        };
        assert!(
            !runtime
                .contains(&other_namespace, &identity)
                .expect("foreign lookup works")
        );
        assert!(
            runtime
                .open_resource(&other_namespace, first.id)
                .await
                .is_err()
        );
        assert!(
            runtime
                .delete_resource(&namespace, first.id)
                .await
                .expect("resource deletes")
        );
        assert!(runtime.open_resource(&namespace, first.id).await.is_err());
        assert!(first_fs.list("").await.is_ok());
        drop(first_fs);
        drop(runtime);

        let reopened = ScopedFileSystemRuntime::new(root.clone(), 1_u8).expect("runtime reopens");
        assert_eq!(
            reopened
                .list_resources(&namespace)
                .await
                .expect("catalog reopens")
                .len(),
            1
        );
        let _ = fs::remove_dir_all(root);
    }
}

fn storage_namespace_id(scope: &impl StorageNamespace) -> Result<StorageNamespaceId, String> {
    let user_sub = scope.user_sub();
    if user_sub.is_empty()
        || matches!(user_sub, "." | "..")
        || !user_sub
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err("invalid user subject".to_owned());
    }
    if scope.application_id().is_nil() {
        return Err("invalid application id".to_owned());
    }
    Ok(StorageNamespaceId {
        user_sub: user_sub.to_owned(),
        application_id: scope.application_id(),
    })
}
