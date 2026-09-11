use idp_model::model::Permission;

use crate::ManagementResult;

pub trait PermissionRepo {
    fn list_permissions(
        &self,
        application_id: i64,
        offset: u32,
        limit: u32,
    ) -> impl Future<Output = ManagementResult<Vec<Permission>>> + Send;

    fn create_permission(
        &self,
        application_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> impl Future<Output = ManagementResult<Permission>> + Send;

    fn find_permission_by_id(
        &self,
        application_id: i64,
        permission_id: i64,
    ) -> impl Future<Output = ManagementResult<Option<Permission>>> + Send;

    fn delete_permission_by_id(
        &self,
        application_id: i64,
        permission_id: i64,
    ) -> impl Future<Output = ManagementResult<()>> + Send;

    fn add_permission_to_role(
        &self,
        application_id: i64,
        role_id: i64,
        permission_id: i64,
    ) -> impl Future<Output = ManagementResult<()>> + Send;

    fn remove_permission_from_role(
        &self,
        application_id: i64,
        role_id: i64,
        permission_id: i64,
    ) -> impl Future<Output = ManagementResult<()>> + Send;

    fn list_role_permissions(
        &self,
        application_id: i64,
        role_id: i64,
    ) -> impl Future<Output = ManagementResult<Vec<Permission>>> + Send;
}
