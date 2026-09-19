use std::sync::Arc;

use crate::PermissionRepo;
use crate::{ManagementError, ManagementResult};
use chrono::Utc;
use file_system::{FileSystem, PeerCodec, Storage, Transport};
use idp_model::model::Permission;

use super::json_store::JsonStore;

const PERMISSIONS_FOLDER: &str = "idp/permissions";
const ROLE_PERMISSIONS_FOLDER: &str = "idp/role-permissions";

pub struct FsPermissionRepo<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>> {
    store: JsonStore<S, C, T>,
}

impl<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>> Clone
    for FsPermissionRepo<S, C, T>
{
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
        }
    }
}

impl<S, C, T> FsPermissionRepo<S, C, T>
where
    S: Storage + Send + Sync + 'static,
    S::Error: std::fmt::Display + Send + 'static,
    C: PeerCodec + Send + 'static,
    C::Error: Send + 'static,
    C::PeerId: Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: 'static,
{
    #[must_use]
    pub fn new(file_system: Arc<FileSystem<S, C, T>>) -> Self {
        Self {
            store: JsonStore::new(file_system),
        }
    }

    async fn permissions(&self) -> ManagementResult<Vec<Permission>> {
        let paths = self.store.list(PERMISSIONS_FOLDER).await?;
        let mut permissions = Vec::with_capacity(paths.len());
        for path in paths {
            permissions.push(self.store.read(&path).await?);
        }
        permissions.sort_by_key(|permission: &Permission| permission.name.clone());
        Ok(permissions)
    }

    async fn next_id(&self) -> ManagementResult<idp_model::model::Id> {
        // ponytail: scan records; replace with a replicated ID allocator only if permission creation is hot.
        let ids = self
            .permissions()
            .await?
            .into_iter()
            .map(|permission| permission.id)
            .collect::<std::collections::BTreeSet<_>>();
        loop {
            let mut bytes = [0_u8; 16];
            getrandom::fill(&mut bytes).map_err(ManagementError::other)?;
            let id = idp_model::model::Id::from_bytes(bytes);
            if !ids.contains(&id) {
                return Ok(id);
            }
        }
    }

    async fn permission(
        &self,
        permission_id: idp_model::model::Id,
    ) -> ManagementResult<Option<Permission>> {
        match self.store.read(&permission_path(permission_id)).await {
            Ok(permission) => Ok(Some(permission)),
            Err(ManagementError::InvalidInput(message)) if message.contains("not found") => {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }
}

impl<S, C, T> PermissionRepo for FsPermissionRepo<S, C, T>
where
    S: Storage + Send + Sync + 'static,
    S::Error: std::fmt::Display + Send + 'static,
    C: PeerCodec + Send + 'static,
    C::Error: Send + 'static,
    C::PeerId: Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: 'static,
{
    async fn list_permissions(
        &self,
        application_id: idp_model::model::Id,
        offset: u32,
        limit: u32,
    ) -> ManagementResult<Vec<Permission>> {
        Ok(self
            .permissions()
            .await?
            .into_iter()
            .filter(|permission| permission.application_id == application_id)
            .skip(offset as usize)
            .take(limit as usize)
            .collect())
    }

    async fn create_permission(
        &self,
        application_id: idp_model::model::Id,
        name: &str,
        description: Option<&str>,
    ) -> ManagementResult<Permission> {
        let now = Utc::now();
        let permission = Permission {
            id: self.next_id().await?,
            application_id,
            name: name.into(),
            description: description.map(str::to_owned),
            created_at: now,
            updated_at: now,
        };
        self.store
            .write(&permission_path(permission.id), &permission)
            .await?;
        Ok(permission)
    }

    async fn find_permission_by_id(
        &self,
        application_id: idp_model::model::Id,
        permission_id: idp_model::model::Id,
    ) -> ManagementResult<Option<Permission>> {
        Ok(self
            .permission(permission_id)
            .await?
            .filter(|permission| permission.application_id == application_id))
    }

    async fn delete_permission_by_id(
        &self,
        application_id: idp_model::model::Id,
        permission_id: idp_model::model::Id,
    ) -> ManagementResult<()> {
        if self
            .find_permission_by_id(application_id, permission_id)
            .await?
            .is_none()
        {
            return Err(ManagementError::InvalidInput(
                "permission was not found".into(),
            ));
        }
        self.store.delete(&permission_path(permission_id)).await
    }

    async fn add_permission_to_role(
        &self,
        application_id: idp_model::model::Id,
        role_id: idp_model::model::Id,
        permission_id: idp_model::model::Id,
    ) -> ManagementResult<()> {
        if self
            .find_permission_by_id(application_id, permission_id)
            .await?
            .is_none()
        {
            return Err(ManagementError::InvalidInput(
                "permission was not found".into(),
            ));
        }
        self.store
            .write(&role_permission_path(role_id, permission_id), &())
            .await
    }

    async fn remove_permission_from_role(
        &self,
        application_id: idp_model::model::Id,
        role_id: idp_model::model::Id,
        permission_id: idp_model::model::Id,
    ) -> ManagementResult<()> {
        if self
            .find_permission_by_id(application_id, permission_id)
            .await?
            .is_none()
        {
            return Err(ManagementError::InvalidInput(
                "permission was not found".into(),
            ));
        }
        self.store
            .delete(&role_permission_path(role_id, permission_id))
            .await
    }

    async fn list_role_permissions(
        &self,
        application_id: idp_model::model::Id,
        role_id: idp_model::model::Id,
    ) -> ManagementResult<Vec<Permission>> {
        let paths = self.store.list(&role_permissions_folder(role_id)).await?;
        let mut permissions = Vec::with_capacity(paths.len());
        for path in paths {
            let permission_id = path
                .rsplit_once('/')
                .and_then(|(_, name)| name.strip_suffix(".json"))
                .and_then(|id| id.parse().ok())
                .ok_or_else(|| {
                    ManagementError::InvalidInput("invalid role permission path".into())
                })?;
            if let Some(permission) = self
                .find_permission_by_id(application_id, permission_id)
                .await?
            {
                permissions.push(permission);
            }
        }
        permissions.sort_by_key(|permission| permission.name.clone());
        Ok(permissions)
    }
}

fn permission_path(id: idp_model::model::Id) -> String {
    format!("{PERMISSIONS_FOLDER}/{id}.json")
}

fn role_permissions_folder(role_id: idp_model::model::Id) -> String {
    format!("{ROLE_PERMISSIONS_FOLDER}/{role_id}")
}

fn role_permission_path(
    role_id: idp_model::model::Id,
    permission_id: idp_model::model::Id,
) -> String {
    format!("{}/{permission_id}.json", role_permissions_folder(role_id))
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, sync::Arc};

    use crate::PermissionRepo;
    use file_system::{FileSystem, InMemoryStorage, MemoryTransport, PeerCodec};

    use super::FsPermissionRepo;

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

    #[tokio::test]
    async fn creates_links_and_lists_role_permissions() {
        let (transport, _) = MemoryTransport::pair(Peer, Peer);
        let file_system: Arc<FileSystem<InMemoryStorage, Peer, MemoryTransport<Peer>>> = Arc::new(
            FileSystem::new(InMemoryStorage::new(), Peer, transport)
                .await
                .unwrap(),
        );
        let repo = FsPermissionRepo::new(file_system);
        let permission = repo
            .create_permission(1, "users.read", Some("Read users"))
            .await
            .unwrap();

        repo.add_permission_to_role(1, 2, permission.id)
            .await
            .unwrap();

        let permissions = repo.list_role_permissions(1, 2).await.unwrap();
        assert_eq!(permissions.len(), 1);
        assert_eq!(permissions[0].id, permission.id);
        assert_eq!(permissions[0].application_id, permission.application_id);
        assert_eq!(permissions[0].name, permission.name);
        assert_eq!(permissions[0].description, permission.description);
    }
}
