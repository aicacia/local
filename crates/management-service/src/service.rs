use alloc::{string::String, vec::Vec};

use idp_model::model::{Application, Permission, Role};
use idp_service::repo::ApplicationRepo;

use crate::{ManagementError, PermissionRepo, RoleRepo};

pub const MANAGEMENT_APPLICATION_URI: &str = "idp-management";

pub struct ManagementService<A, P, R> {
    application_repo: A,
    permission_repo: P,
    role_repo: R,
}

impl<A, P, R> ManagementService<A, P, R>
where
    A: ApplicationRepo,
    P: PermissionRepo,
    R: RoleRepo,
{
    pub fn new(application_repo: A, permission_repo: P, role_repo: R) -> Self {
        Self {
            application_repo,
            permission_repo,
            role_repo,
        }
    }

    pub async fn has_user_application_permission(
        &self,
        user_id: idp_model::model::Id,
        permission_name: &str,
    ) -> Result<bool, ManagementError> {
        self.role_repo
            .has_user_client_permission(user_id, MANAGEMENT_APPLICATION_URI, permission_name)
            .await
    }

    pub async fn list_roles(
        &self,
        application_id: idp_model::model::Id,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<Role>, ManagementError> {
        self.role_repo
            .list_roles(application_id, offset, limit)
            .await
    }

    pub async fn create_role(
        &self,
        application_id: idp_model::model::Id,
        name: &str,
        description: Option<&str>,
    ) -> Result<Role, ManagementError> {
        self.role_repo
            .create_role(application_id, name, description)
            .await
    }

    pub async fn find_role_by_id(
        &self,
        application_id: idp_model::model::Id,
        role_id: idp_model::model::Id,
    ) -> Result<Option<Role>, ManagementError> {
        self.role_repo
            .find_role_by_id(application_id, role_id)
            .await
    }

    pub async fn delete_role_by_id(
        &self,
        application_id: idp_model::model::Id,
        role_id: idp_model::model::Id,
    ) -> Result<(), ManagementError> {
        self.role_repo
            .delete_role_by_id(application_id, role_id)
            .await
    }

    pub async fn add_role_to_user(
        &self,
        application_id: idp_model::model::Id,
        user_id: idp_model::model::Id,
        role_id: idp_model::model::Id,
    ) -> Result<(), ManagementError> {
        self.role_repo
            .add_role_to_user(application_id, user_id, role_id)
            .await
    }

    pub async fn remove_role_from_user(
        &self,
        application_id: idp_model::model::Id,
        user_id: idp_model::model::Id,
        role_id: idp_model::model::Id,
    ) -> Result<(), ManagementError> {
        self.role_repo
            .remove_role_from_user(application_id, user_id, role_id)
            .await
    }

    pub async fn list_user_roles(
        &self,
        application_id: idp_model::model::Id,
        user_id: idp_model::model::Id,
    ) -> Result<Vec<Role>, ManagementError> {
        self.role_repo
            .list_user_roles(application_id, user_id)
            .await
    }

    pub async fn list_user_roles_across_applications(
        &self,
        user_id: idp_model::model::Id,
    ) -> Result<Vec<Role>, ManagementError> {
        self.role_repo
            .list_user_roles_across_applications(user_id)
            .await
    }

    pub async fn list_permissions(
        &self,
        application_id: idp_model::model::Id,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<Permission>, ManagementError> {
        self.permission_repo
            .list_permissions(application_id, offset, limit)
            .await
    }

    pub async fn create_permission(
        &self,
        application_id: idp_model::model::Id,
        name: &str,
        description: Option<&str>,
    ) -> Result<Permission, ManagementError> {
        self.permission_repo
            .create_permission(application_id, name, description)
            .await
    }

    pub async fn find_permission_by_id(
        &self,
        application_id: idp_model::model::Id,
        permission_id: idp_model::model::Id,
    ) -> Result<Option<Permission>, ManagementError> {
        self.permission_repo
            .find_permission_by_id(application_id, permission_id)
            .await
    }

    pub async fn delete_permission_by_id(
        &self,
        application_id: idp_model::model::Id,
        permission_id: idp_model::model::Id,
    ) -> Result<(), ManagementError> {
        self.permission_repo
            .delete_permission_by_id(application_id, permission_id)
            .await
    }

    pub async fn list_role_permissions(
        &self,
        application_id: idp_model::model::Id,
        role_id: idp_model::model::Id,
    ) -> Result<Vec<Permission>, ManagementError> {
        self.permission_repo
            .list_role_permissions(application_id, role_id)
            .await
    }

    pub async fn add_permission_to_role(
        &self,
        application_id: idp_model::model::Id,
        role_id: idp_model::model::Id,
        permission_id: idp_model::model::Id,
    ) -> Result<(), ManagementError> {
        self.permission_repo
            .add_permission_to_role(application_id, role_id, permission_id)
            .await
    }

    pub async fn remove_permission_from_role(
        &self,
        application_id: idp_model::model::Id,
        role_id: idp_model::model::Id,
        permission_id: idp_model::model::Id,
    ) -> Result<(), ManagementError> {
        self.permission_repo
            .remove_permission_from_role(application_id, role_id, permission_id)
            .await
    }

    pub async fn list_applications(
        &self,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<Application>, ManagementError> {
        self.application_repo
            .list_applications(offset, limit)
            .await
            .map_err(ManagementError::other)
    }

    pub async fn create_application(
        &self,
        name: String,
        uri: String,
        description: Option<String>,
    ) -> Result<Application, ManagementError> {
        self.application_repo
            .create_application(name, uri, description)
            .await
            .map_err(ManagementError::other)
    }

    pub async fn find_application_by_id(
        &self,
        application_id: idp_model::model::Id,
    ) -> Result<Option<Application>, ManagementError> {
        self.application_repo
            .find_by_id(application_id)
            .await
            .map_err(ManagementError::other)
    }

    pub async fn find_application_by_uri(
        &self,
        application_uri: &str,
    ) -> Result<Option<Application>, ManagementError> {
        self.application_repo
            .find_by_uri(application_uri)
            .await
            .map_err(ManagementError::other)
    }

    pub async fn update_application(
        &self,
        application: Application,
    ) -> Result<Application, ManagementError> {
        self.application_repo
            .update_application(application)
            .await
            .map_err(ManagementError::other)
    }

    pub async fn delete_application_by_id(
        &self,
        application_id: idp_model::model::Id,
    ) -> Result<(), ManagementError> {
        self.application_repo
            .delete_application_by_id(application_id)
            .await
            .map_err(ManagementError::other)
    }
}
