use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use crate::RoleRepo;
use crate::{ManagementError, ManagementResult};
use chrono::Utc;
use file_system::{FileSystem, PeerCodec, Storage, Transport};
use idp_model::model::{Application, Permission, Role};

use super::json_store::JsonStore;

const APPLICATIONS_FOLDER: &str = "idp/applications";
const PERMISSIONS_FOLDER: &str = "idp/permissions";
const ROLE_PERMISSIONS_FOLDER: &str = "idp/role-permissions";
const ROLES_FOLDER: &str = "idp/roles";
const USER_ROLES_FOLDER: &str = "idp/user-roles";

pub struct FsRoleRepo<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>> {
    store: JsonStore<S, C, T>,
}

impl<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>> Clone for FsRoleRepo<S, C, T> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
        }
    }
}

impl<S, C, T> FsRoleRepo<S, C, T>
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

    async fn roles(&self) -> ManagementResult<Vec<Role>> {
        let paths = self.store.list(ROLES_FOLDER).await?;
        let mut roles = Vec::with_capacity(paths.len());
        for path in paths {
            roles.push(self.store.read(&path).await?);
        }
        roles.sort_by_key(|role: &Role| (role.application_id, role.name.clone()));
        Ok(roles)
    }

    async fn next_id(&self) -> ManagementResult<i64> {
        // ponytail: scan records; replace with a replicated ID allocator only if role creation is hot.
        let ids = self
            .roles()
            .await?
            .into_iter()
            .map(|role| role.id)
            .collect::<BTreeSet<_>>();
        loop {
            let mut bytes = [0_u8; 8];
            getrandom::fill(&mut bytes).map_err(ManagementError::other)?;
            let id = i64::from_be_bytes(bytes) & i64::MAX;
            if id != 0 && !ids.contains(&id) {
                return Ok(id);
            }
        }
    }

    async fn role(&self, role_id: i64) -> ManagementResult<Option<Role>> {
        match self.store.read(&role_path(role_id)).await {
            Ok(role) => Ok(Some(role)),
            Err(ManagementError::InvalidInput(message)) if message.contains("not found") => {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    async fn permissions_for_role(
        &self,
        application_id: i64,
        role_id: i64,
    ) -> ManagementResult<Vec<Permission>> {
        let paths = self.store.list(&role_permissions_folder(role_id)).await?;
        let mut permissions = Vec::with_capacity(paths.len());
        for path in paths {
            let Some(permission_id) = path
                .rsplit_once('/')
                .and_then(|(_, name)| name.strip_suffix(".json"))
                .and_then(|id| id.parse().ok())
            else {
                return Err(ManagementError::InvalidInput(
                    "invalid role permission path".into(),
                ));
            };
            if let Some(permission) = self.permission(permission_id).await?
                && permission.application_id == application_id
            {
                permissions.push(permission);
            }
        }
        Ok(permissions)
    }

    async fn permission(&self, permission_id: i64) -> ManagementResult<Option<Permission>> {
        match self.store.read(&permission_path(permission_id)).await {
            Ok(permission) => Ok(Some(permission)),
            Err(ManagementError::InvalidInput(message)) if message.contains("not found") => {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    async fn application_by_uri(&self, uri: &str) -> ManagementResult<Option<Application>> {
        let paths = self.store.list(APPLICATIONS_FOLDER).await?;
        for path in paths {
            let application: Application = self.store.read(&path).await?;
            if application.uri == uri {
                return Ok(Some(application));
            }
        }
        Ok(None)
    }
}

impl<S, C, T> RoleRepo for FsRoleRepo<S, C, T>
where
    S: Storage + Send + Sync + 'static,
    S::Error: std::fmt::Display + Send + 'static,
    C: PeerCodec + Send + 'static,
    C::Error: Send + 'static,
    C::PeerId: Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: 'static,
{
    async fn list_roles(
        &self,
        application_id: i64,
        offset: u32,
        limit: u32,
    ) -> ManagementResult<Vec<Role>> {
        Ok(self
            .roles()
            .await?
            .into_iter()
            .filter(|role| role.application_id == application_id)
            .skip(offset as usize)
            .take(limit as usize)
            .collect())
    }

    async fn create_role(
        &self,
        application_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> ManagementResult<Role> {
        let now = Utc::now();
        let role = Role {
            id: self.next_id().await?,
            application_id,
            name: name.into(),
            description: description.map(str::to_owned),
            created_at: now,
            updated_at: now,
        };
        self.store.write(&role_path(role.id), &role).await?;
        Ok(role)
    }

    async fn find_role_by_id(
        &self,
        application_id: i64,
        role_id: i64,
    ) -> ManagementResult<Option<Role>> {
        Ok(self
            .role(role_id)
            .await?
            .filter(|role| role.application_id == application_id))
    }

    async fn delete_role_by_id(&self, application_id: i64, role_id: i64) -> ManagementResult<()> {
        if self
            .find_role_by_id(application_id, role_id)
            .await?
            .is_none()
        {
            return Err(ManagementError::InvalidInput("role was not found".into()));
        }
        self.store.delete(&role_path(role_id)).await
    }

    async fn add_role_to_user(
        &self,
        application_id: i64,
        user_id: i64,
        role_id: i64,
    ) -> ManagementResult<()> {
        if self
            .find_role_by_id(application_id, role_id)
            .await?
            .is_none()
        {
            return Err(ManagementError::InvalidInput("role was not found".into()));
        }
        self.store
            .write(&user_role_path(user_id, role_id), &())
            .await
    }

    async fn remove_role_from_user(
        &self,
        application_id: i64,
        user_id: i64,
        role_id: i64,
    ) -> ManagementResult<()> {
        if self
            .find_role_by_id(application_id, role_id)
            .await?
            .is_none()
        {
            return Err(ManagementError::InvalidInput("role was not found".into()));
        }
        self.store.delete(&user_role_path(user_id, role_id)).await
    }

    async fn list_user_roles(
        &self,
        application_id: i64,
        user_id: i64,
    ) -> ManagementResult<Vec<Role>> {
        let paths = self.store.list(&user_roles_folder(user_id)).await?;
        let mut roles = Vec::with_capacity(paths.len());
        for path in paths {
            let Some(role_id) = id_from_path(&path) else {
                return Err(ManagementError::InvalidInput(
                    "invalid user role path".into(),
                ));
            };
            if let Some(role) = self.find_role_by_id(application_id, role_id).await? {
                roles.push(role);
            }
        }
        roles.sort_by_key(|role| role.name.clone());
        Ok(roles)
    }

    async fn list_user_roles_across_applications(
        &self,
        user_id: i64,
    ) -> ManagementResult<Vec<Role>> {
        let paths = self.store.list(&user_roles_folder(user_id)).await?;
        let mut roles = Vec::with_capacity(paths.len());
        for path in paths {
            let Some(role_id) = id_from_path(&path) else {
                return Err(ManagementError::InvalidInput(
                    "invalid user role path".into(),
                ));
            };
            if let Some(role) = self.role(role_id).await? {
                roles.push(role);
            }
        }
        roles.sort_by_key(|role| (role.application_id, role.name.clone()));
        Ok(roles)
    }

    async fn list_user_permissions(
        &self,
        application_id: i64,
        user_id: i64,
    ) -> ManagementResult<Vec<Permission>> {
        let mut permissions = BTreeMap::new();
        for role in self.list_user_roles(application_id, user_id).await? {
            for permission in self.permissions_for_role(application_id, role.id).await? {
                permissions.insert((permission.name.clone(), permission.id), permission);
            }
        }
        Ok(permissions.into_values().collect())
    }

    async fn has_user_client_permission(
        &self,
        user_id: i64,
        application_uri: &str,
        permission_name: &str,
    ) -> ManagementResult<bool> {
        let Some(application) = self.application_by_uri(application_uri).await? else {
            return Ok(false);
        };
        Ok(self
            .list_user_permissions(application.id, user_id)
            .await?
            .into_iter()
            .any(|permission| permission_matches(&permission.name, permission_name)))
    }
}

fn permission_matches(granted: &str, required: &str) -> bool {
    if granted == "*" || granted == required {
        return true;
    }
    let Some(prefix) = granted.strip_suffix('*') else {
        return false;
    };
    if required.starts_with(prefix) {
        return true;
    }
    prefix
        .strip_suffix(':')
        .is_some_and(|prefix| required.starts_with(&format!("{prefix}.")))
        || prefix
            .strip_suffix('.')
            .is_some_and(|prefix| required.starts_with(&format!("{prefix}:")))
}

fn id_from_path(path: &str) -> Option<i64> {
    path.rsplit_once('/')?.1.strip_suffix(".json")?.parse().ok()
}

fn permission_path(id: i64) -> String {
    format!("{PERMISSIONS_FOLDER}/{id}.json")
}

fn role_path(id: i64) -> String {
    format!("{ROLES_FOLDER}/{id}.json")
}

fn role_permissions_folder(role_id: i64) -> String {
    format!("{ROLE_PERMISSIONS_FOLDER}/{role_id}")
}

fn user_roles_folder(user_id: i64) -> String {
    format!("{USER_ROLES_FOLDER}/{user_id}")
}

fn user_role_path(user_id: i64, role_id: i64) -> String {
    format!("{}/{role_id}.json", user_roles_folder(user_id))
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, sync::Arc};

    use chrono::Utc;
    use file_system::{FileSystem, InMemoryStorage, MemoryTransport, PeerCodec};
    use idp_model::model::Application;

    use crate::{PermissionRepo, RoleRepo, fs::FsPermissionRepo};

    use super::super::json_store::JsonStore;
    use super::FsRoleRepo;

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
    async fn resolves_user_permissions_for_an_application_uri() {
        let (transport, _) = MemoryTransport::pair(Peer, Peer);
        let file_system: Arc<FileSystem<InMemoryStorage, Peer, MemoryTransport<Peer>>> = Arc::new(
            FileSystem::new(InMemoryStorage::new(), Peer, transport)
                .await
                .unwrap(),
        );
        let applications = JsonStore::new(Arc::clone(&file_system));
        let permissions = FsPermissionRepo::new(Arc::clone(&file_system));
        let roles = FsRoleRepo::new(file_system);
        let now = Utc::now();
        let application = Application {
            id: 1,
            name: "Test".into(),
            uri: "test".into(),
            description: None,
            created_at: now,
            updated_at: now,
        };
        applications
            .write("idp/applications/1.json", &application)
            .await
            .unwrap();
        let permission = permissions
            .create_permission(application.id, "users.*", None)
            .await
            .unwrap();
        let role = roles
            .create_role(application.id, "admin", None)
            .await
            .unwrap();

        permissions
            .add_permission_to_role(application.id, role.id, permission.id)
            .await
            .unwrap();
        roles
            .add_role_to_user(application.id, 1, role.id)
            .await
            .unwrap();

        let user_permissions = roles
            .list_user_permissions(application.id, 1)
            .await
            .unwrap();
        assert_eq!(user_permissions.len(), 1);
        assert_eq!(user_permissions[0].id, permission.id);
        assert!(
            roles
                .has_user_client_permission(1, "test", "users.read")
                .await
                .unwrap()
        );
    }
}
