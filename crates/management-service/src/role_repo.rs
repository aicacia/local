use idp_model::model::{Permission, Role};

use crate::ManagementResult;

pub trait RoleRepo {
    fn list_roles(
        &self,
        application_id: i64,
        offset: u32,
        limit: u32,
    ) -> impl Future<Output = ManagementResult<Vec<Role>>> + Send;

    fn create_role(
        &self,
        application_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> impl Future<Output = ManagementResult<Role>> + Send;

    fn find_role_by_id(
        &self,
        application_id: i64,
        role_id: i64,
    ) -> impl Future<Output = ManagementResult<Option<Role>>> + Send;

    fn delete_role_by_id(
        &self,
        application_id: i64,
        role_id: i64,
    ) -> impl Future<Output = ManagementResult<()>> + Send;

    fn add_role_to_user(
        &self,
        application_id: i64,
        user_id: i64,
        role_id: i64,
    ) -> impl Future<Output = ManagementResult<()>> + Send;

    fn remove_role_from_user(
        &self,
        application_id: i64,
        user_id: i64,
        role_id: i64,
    ) -> impl Future<Output = ManagementResult<()>> + Send;

    fn list_user_roles(
        &self,
        application_id: i64,
        user_id: i64,
    ) -> impl Future<Output = ManagementResult<Vec<Role>>> + Send;

    fn list_user_roles_across_applications(
        &self,
        user_id: i64,
    ) -> impl Future<Output = ManagementResult<Vec<Role>>> + Send;

    fn list_user_permissions(
        &self,
        application_id: i64,
        user_id: i64,
    ) -> impl Future<Output = ManagementResult<Vec<Permission>>> + Send;

    fn has_user_client_permission(
        &self,
        user_id: i64,
        application_uri: &str,
        permission_name: &str,
    ) -> impl Future<Output = ManagementResult<bool>> + Send;
}
