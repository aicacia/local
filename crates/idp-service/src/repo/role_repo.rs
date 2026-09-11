use idp_model::model::{Permission, Role};

use crate::repo::RepoResult;

pub trait RoleRepo {
    fn list_roles(
        &self,
        application_id: i64,
        offset: u32,
        limit: u32,
    ) -> impl Future<Output = RepoResult<Vec<Role>>>;

    fn create_role(
        &self,
        application_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> impl Future<Output = RepoResult<Role>>;

    fn find_role_by_id(
        &self,
        application_id: i64,
        role_id: i64,
    ) -> impl Future<Output = RepoResult<Option<Role>>>;

    fn delete_role_by_id(
        &self,
        application_id: i64,
        role_id: i64,
    ) -> impl Future<Output = RepoResult<()>>;

    fn add_role_to_user(
        &self,
        application_id: i64,
        user_id: i64,
        role_id: i64,
    ) -> impl Future<Output = RepoResult<()>>;

    fn remove_role_from_user(
        &self,
        application_id: i64,
        user_id: i64,
        role_id: i64,
    ) -> impl Future<Output = RepoResult<()>>;

    fn list_user_roles(
        &self,
        application_id: i64,
        user_id: i64,
    ) -> impl Future<Output = RepoResult<Vec<Role>>>;

    fn list_user_roles_across_applications(
        &self,
        user_id: i64,
    ) -> impl Future<Output = RepoResult<Vec<Role>>>;

    fn list_user_permissions(
        &self,
        application_id: i64,
        user_id: i64,
    ) -> impl Future<Output = RepoResult<Vec<Permission>>>;

    fn has_user_client_permission(
        &self,
        user_id: i64,
        application_uri: &str,
        permission_name: &str,
    ) -> impl Future<Output = RepoResult<bool>>;
}
