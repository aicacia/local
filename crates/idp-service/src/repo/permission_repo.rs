use idp_model::model::Permission;

use crate::repo::RepoResult;

pub trait PermissionRepo {
    fn list_permissions(
        &self,
        application_id: i64,
        offset: u32,
        limit: u32,
    ) -> impl Future<Output = RepoResult<Vec<Permission>>>;

    fn create_permission(
        &self,
        application_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> impl Future<Output = RepoResult<Permission>>;

    fn find_permission_by_id(
        &self,
        application_id: i64,
        permission_id: i64,
    ) -> impl Future<Output = RepoResult<Option<Permission>>>;

    fn delete_permission_by_id(
        &self,
        application_id: i64,
        permission_id: i64,
    ) -> impl Future<Output = RepoResult<()>>;

    fn add_permission_to_role(
        &self,
        application_id: i64,
        role_id: i64,
        permission_id: i64,
    ) -> impl Future<Output = RepoResult<()>>;

    fn remove_permission_from_role(
        &self,
        application_id: i64,
        role_id: i64,
        permission_id: i64,
    ) -> impl Future<Output = RepoResult<()>>;

    fn list_role_permissions(
        &self,
        application_id: i64,
        role_id: i64,
    ) -> impl Future<Output = RepoResult<Vec<Permission>>>;
}
