use core::{future::Future, pin::Pin};
use std::sync::Arc;

use idp_model::model::{Application, Permission, Role};
use idp_server::GlobalIdentityReadGateSlot;
use idp_service::libsql::{
    LibSqlApplicationRepo, LibSqlClientRepo, LibSqlKeyRepo, LibSqlOAuth2AuthorizationCodeRepo,
    LibSqlOAuth2UserConsentRepo, LibSqlUserRepo,
};
use idp_service::{oauth2::OAuth2Service, repo::ApplicationRepo};
use libsql::Database;
use management_service::{ManagementResult, ManagementService, PermissionRepo, RoleRepo};

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = ManagementResult<T>> + Send + 'a>>;

pub(crate) type ManagementRouterService = dyn ManagementRouterBackend;

pub(crate) trait ManagementRouterBackend: Send + Sync {
    fn has_user_application_permission(
        &self,
        user_id: i64,
        permission_name: &str,
    ) -> BoxFuture<'_, bool>;
    fn list_roles(&self, application_id: i64, offset: u32, limit: u32) -> BoxFuture<'_, Vec<Role>>;
    fn create_role(
        &self,
        application_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> BoxFuture<'_, Role>;
    fn find_role_by_id(&self, application_id: i64, role_id: i64) -> BoxFuture<'_, Option<Role>>;
    fn delete_role_by_id(&self, application_id: i64, role_id: i64) -> BoxFuture<'_, ()>;
    fn add_role_to_user(
        &self,
        application_id: i64,
        user_id: i64,
        role_id: i64,
    ) -> BoxFuture<'_, ()>;
    fn remove_role_from_user(
        &self,
        application_id: i64,
        user_id: i64,
        role_id: i64,
    ) -> BoxFuture<'_, ()>;
    fn list_user_roles(&self, application_id: i64, user_id: i64) -> BoxFuture<'_, Vec<Role>>;
    fn list_user_roles_across_applications(&self, user_id: i64) -> BoxFuture<'_, Vec<Role>>;
    fn list_permissions(
        &self,
        application_id: i64,
        offset: u32,
        limit: u32,
    ) -> BoxFuture<'_, Vec<Permission>>;
    fn create_permission(
        &self,
        application_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> BoxFuture<'_, Permission>;
    fn find_permission_by_id(
        &self,
        application_id: i64,
        permission_id: i64,
    ) -> BoxFuture<'_, Option<Permission>>;
    fn delete_permission_by_id(&self, application_id: i64, permission_id: i64)
    -> BoxFuture<'_, ()>;
    fn list_role_permissions(
        &self,
        application_id: i64,
        role_id: i64,
    ) -> BoxFuture<'_, Vec<Permission>>;
    fn add_permission_to_role(
        &self,
        application_id: i64,
        role_id: i64,
        permission_id: i64,
    ) -> BoxFuture<'_, ()>;
    fn remove_permission_from_role(
        &self,
        application_id: i64,
        role_id: i64,
        permission_id: i64,
    ) -> BoxFuture<'_, ()>;
    fn list_applications(&self, offset: u32, limit: u32) -> BoxFuture<'_, Vec<Application>>;
    fn create_application(
        &self,
        name: String,
        uri: String,
        description: Option<String>,
    ) -> BoxFuture<'_, Application>;
    fn find_application_by_id(&self, application_id: i64) -> BoxFuture<'_, Option<Application>>;
    fn update_application(&self, application: Application) -> BoxFuture<'_, Application>;
    fn delete_application_by_id(&self, application_id: i64) -> BoxFuture<'_, ()>;
}

impl<A, P, R> ManagementRouterBackend for ManagementService<A, P, R>
where
    A: ApplicationRepo + Send + Sync + 'static,
    P: PermissionRepo + Send + Sync + 'static,
    R: RoleRepo + Send + Sync + 'static,
{
    fn has_user_application_permission(
        &self,
        user_id: i64,
        permission_name: &str,
    ) -> BoxFuture<'_, bool> {
        let permission_name = permission_name.to_owned();
        Box::pin(async move {
            ManagementService::has_user_application_permission(self, user_id, &permission_name)
                .await
        })
    }

    fn list_roles(&self, application_id: i64, offset: u32, limit: u32) -> BoxFuture<'_, Vec<Role>> {
        Box::pin(ManagementService::list_roles(
            self,
            application_id,
            offset,
            limit,
        ))
    }

    fn create_role(
        &self,
        application_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> BoxFuture<'_, Role> {
        let name = name.to_owned();
        let description = description.map(str::to_owned);
        Box::pin(async move {
            ManagementService::create_role(self, application_id, &name, description.as_deref())
                .await
        })
    }

    fn find_role_by_id(&self, application_id: i64, role_id: i64) -> BoxFuture<'_, Option<Role>> {
        Box::pin(ManagementService::find_role_by_id(
            self,
            application_id,
            role_id,
        ))
    }

    fn delete_role_by_id(&self, application_id: i64, role_id: i64) -> BoxFuture<'_, ()> {
        Box::pin(ManagementService::delete_role_by_id(
            self,
            application_id,
            role_id,
        ))
    }

    fn add_role_to_user(
        &self,
        application_id: i64,
        user_id: i64,
        role_id: i64,
    ) -> BoxFuture<'_, ()> {
        Box::pin(ManagementService::add_role_to_user(
            self,
            application_id,
            user_id,
            role_id,
        ))
    }

    fn remove_role_from_user(
        &self,
        application_id: i64,
        user_id: i64,
        role_id: i64,
    ) -> BoxFuture<'_, ()> {
        Box::pin(ManagementService::remove_role_from_user(
            self,
            application_id,
            user_id,
            role_id,
        ))
    }

    fn list_user_roles(&self, application_id: i64, user_id: i64) -> BoxFuture<'_, Vec<Role>> {
        Box::pin(ManagementService::list_user_roles(
            self,
            application_id,
            user_id,
        ))
    }

    fn list_user_roles_across_applications(&self, user_id: i64) -> BoxFuture<'_, Vec<Role>> {
        Box::pin(ManagementService::list_user_roles_across_applications(
            self, user_id,
        ))
    }

    fn list_permissions(
        &self,
        application_id: i64,
        offset: u32,
        limit: u32,
    ) -> BoxFuture<'_, Vec<Permission>> {
        Box::pin(ManagementService::list_permissions(
            self,
            application_id,
            offset,
            limit,
        ))
    }

    fn create_permission(
        &self,
        application_id: i64,
        name: &str,
        description: Option<&str>,
    ) -> BoxFuture<'_, Permission> {
        let name = name.to_owned();
        let description = description.map(str::to_owned);
        Box::pin(async move {
            ManagementService::create_permission(
                self,
                application_id,
                &name,
                description.as_deref(),
            )
            .await
        })
    }

    fn find_permission_by_id(
        &self,
        application_id: i64,
        permission_id: i64,
    ) -> BoxFuture<'_, Option<Permission>> {
        Box::pin(ManagementService::find_permission_by_id(
            self,
            application_id,
            permission_id,
        ))
    }

    fn delete_permission_by_id(
        &self,
        application_id: i64,
        permission_id: i64,
    ) -> BoxFuture<'_, ()> {
        Box::pin(ManagementService::delete_permission_by_id(
            self,
            application_id,
            permission_id,
        ))
    }

    fn list_role_permissions(
        &self,
        application_id: i64,
        role_id: i64,
    ) -> BoxFuture<'_, Vec<Permission>> {
        Box::pin(ManagementService::list_role_permissions(
            self,
            application_id,
            role_id,
        ))
    }

    fn add_permission_to_role(
        &self,
        application_id: i64,
        role_id: i64,
        permission_id: i64,
    ) -> BoxFuture<'_, ()> {
        Box::pin(ManagementService::add_permission_to_role(
            self,
            application_id,
            role_id,
            permission_id,
        ))
    }

    fn remove_permission_from_role(
        &self,
        application_id: i64,
        role_id: i64,
        permission_id: i64,
    ) -> BoxFuture<'_, ()> {
        Box::pin(ManagementService::remove_permission_from_role(
            self,
            application_id,
            role_id,
            permission_id,
        ))
    }

    fn list_applications(&self, offset: u32, limit: u32) -> BoxFuture<'_, Vec<Application>> {
        Box::pin(ManagementService::list_applications(self, offset, limit))
    }

    fn create_application(
        &self,
        name: String,
        uri: String,
        description: Option<String>,
    ) -> BoxFuture<'_, Application> {
        Box::pin(ManagementService::create_application(
            self,
            name,
            uri,
            description,
        ))
    }

    fn find_application_by_id(&self, application_id: i64) -> BoxFuture<'_, Option<Application>> {
        Box::pin(ManagementService::find_application_by_id(
            self,
            application_id,
        ))
    }

    fn update_application(&self, application: Application) -> BoxFuture<'_, Application> {
        Box::pin(ManagementService::update_application(self, application))
    }

    fn delete_application_by_id(&self, application_id: i64) -> BoxFuture<'_, ()> {
        Box::pin(ManagementService::delete_application_by_id(
            self,
            application_id,
        ))
    }
}

#[derive(Clone)]
pub struct RouterState {
    pub(crate) api_base_uri: String,
    pub(crate) database: Arc<Database>,
    pub(crate) management_service: Arc<ManagementRouterService>,
    pub(crate) global_identity_read_gate: Arc<GlobalIdentityReadGateSlot>,
    pub(crate) oauth2_service: Arc<
        OAuth2Service<
            LibSqlApplicationRepo,
            LibSqlClientRepo,
            LibSqlOAuth2AuthorizationCodeRepo,
            LibSqlUserRepo,
            LibSqlOAuth2UserConsentRepo,
            LibSqlKeyRepo,
        >,
    >,
}

impl RouterState {
    pub fn new<A, P, R>(
        api_base_uri: impl Into<String>,
        database: Arc<Database>,
        global_identity_read_gate: Arc<GlobalIdentityReadGateSlot>,
        management_service: Arc<ManagementService<A, P, R>>,
        oauth2_service: Arc<
            OAuth2Service<
                LibSqlApplicationRepo,
                LibSqlClientRepo,
                LibSqlOAuth2AuthorizationCodeRepo,
                LibSqlUserRepo,
                LibSqlOAuth2UserConsentRepo,
                LibSqlKeyRepo,
            >,
        >,
    ) -> Self
    where
        A: ApplicationRepo + Send + Sync + 'static,
        P: PermissionRepo + Send + Sync + 'static,
        R: RoleRepo + Send + Sync + 'static,
    {
        Self {
            api_base_uri: api_base_uri.into(),
            database,
            global_identity_read_gate,
            management_service,
            oauth2_service,
        }
    }
}
