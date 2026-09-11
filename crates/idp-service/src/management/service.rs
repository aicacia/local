use idp_model::model::{Application, Permission, Role};

use crate::repo::{
    ApplicationRepo, LibSqlApplicationRepo, LibSqlPermissionRepo, LibSqlRoleRepo, PermissionRepo,
    RepoError, RoleRepo,
};

pub const MANAGEMENT_APPLICATION_URI: &str = "idp-management";

pub struct ManagementService {
    application_repo: LibSqlApplicationRepo,
    permission_repo: LibSqlPermissionRepo,
    role_repo: LibSqlRoleRepo,
}

impl ManagementService {
    pub fn new(
        application_repo: LibSqlApplicationRepo,
        permission_repo: LibSqlPermissionRepo,
        role_repo: LibSqlRoleRepo,
    ) -> Self {
        Self {
            application_repo,
            permission_repo,
            role_repo,
        }
    }

    pub async fn has_user_application_permission(
        &self,
        user_id: i64,
        permission_name: &str,
    ) -> Result<bool, RepoError> {
        self.role_repo
            .has_user_client_permission(user_id, MANAGEMENT_APPLICATION_URI, permission_name)
            .await
    }

    pub async fn list_roles(
        &self,
        application_id: i64,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<Role>, RepoError> {
        self.role_repo
            .list_roles(application_id, offset, limit)
            .await
    }

    pub async fn create_role(
        &self,
        application_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> Result<Role, RepoError> {
        self.role_repo
            .create_role(application_id, name, description)
            .await
    }

    pub async fn find_role_by_id(
        &self,
        application_id: i64,
        role_id: i64,
    ) -> Result<Option<Role>, RepoError> {
        self.role_repo
            .find_role_by_id(application_id, role_id)
            .await
    }

    pub async fn delete_role_by_id(
        &self,
        application_id: i64,
        role_id: i64,
    ) -> Result<(), RepoError> {
        self.role_repo
            .delete_role_by_id(application_id, role_id)
            .await
    }

    pub async fn add_role_to_user(
        &self,
        application_id: i64,
        user_id: i64,
        role_id: i64,
    ) -> Result<(), RepoError> {
        self.role_repo
            .add_role_to_user(application_id, user_id, role_id)
            .await
    }

    pub async fn remove_role_from_user(
        &self,
        application_id: i64,
        user_id: i64,
        role_id: i64,
    ) -> Result<(), RepoError> {
        self.role_repo
            .remove_role_from_user(application_id, user_id, role_id)
            .await
    }

    pub async fn list_user_roles(
        &self,
        application_id: i64,
        user_id: i64,
    ) -> Result<Vec<Role>, RepoError> {
        self.role_repo
            .list_user_roles(application_id, user_id)
            .await
    }

    pub async fn list_user_roles_across_applications(
        &self,
        user_id: i64,
    ) -> Result<Vec<Role>, RepoError> {
        self.role_repo
            .list_user_roles_across_applications(user_id)
            .await
    }

    pub async fn list_permissions(
        &self,
        application_id: i64,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<Permission>, RepoError> {
        self.permission_repo
            .list_permissions(application_id, offset, limit)
            .await
    }

    pub async fn create_permission(
        &self,
        application_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> Result<Permission, RepoError> {
        self.permission_repo
            .create_permission(application_id, name, description)
            .await
    }

    pub async fn find_permission_by_id(
        &self,
        application_id: i64,
        permission_id: i64,
    ) -> Result<Option<Permission>, RepoError> {
        self.permission_repo
            .find_permission_by_id(application_id, permission_id)
            .await
    }

    pub async fn delete_permission_by_id(
        &self,
        application_id: i64,
        permission_id: i64,
    ) -> Result<(), RepoError> {
        self.permission_repo
            .delete_permission_by_id(application_id, permission_id)
            .await
    }

    pub async fn list_role_permissions(
        &self,
        application_id: i64,
        role_id: i64,
    ) -> Result<Vec<Permission>, RepoError> {
        self.permission_repo
            .list_role_permissions(application_id, role_id)
            .await
    }

    pub async fn add_permission_to_role(
        &self,
        application_id: i64,
        role_id: i64,
        permission_id: i64,
    ) -> Result<(), RepoError> {
        self.permission_repo
            .add_permission_to_role(application_id, role_id, permission_id)
            .await
    }

    pub async fn remove_permission_from_role(
        &self,
        application_id: i64,
        role_id: i64,
        permission_id: i64,
    ) -> Result<(), RepoError> {
        self.permission_repo
            .remove_permission_from_role(application_id, role_id, permission_id)
            .await
    }

    pub async fn list_applications(
        &self,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<Application>, RepoError> {
        self.application_repo.list_applications(offset, limit).await
    }

    pub async fn create_application(
        &self,
        name: String,
        uri: String,
        description: Option<String>,
    ) -> Result<Application, RepoError> {
        self.application_repo
            .create_application(name, uri, description)
            .await
    }

    pub async fn find_application_by_id(
        &self,
        application_id: i64,
    ) -> Result<Option<Application>, RepoError> {
        self.application_repo.find_by_id(application_id).await
    }

    pub async fn find_application_by_uri(
        &self,
        application_uri: &str,
    ) -> Result<Option<Application>, RepoError> {
        self.application_repo.find_by_uri(application_uri).await
    }

    pub async fn update_application(
        &self,
        application: Application,
    ) -> Result<Application, RepoError> {
        self.application_repo.update_application(application).await
    }

    pub async fn delete_application_by_id(&self, application_id: i64) -> Result<(), RepoError> {
        self.application_repo
            .delete_application_by_id(application_id)
            .await
    }
}
