use std::sync::Arc;

use idp_server::GlobalIdentityReadGateSlot;
use idp_service::libsql::{
    LibSqlApplicationRepo, LibSqlClientRepo, LibSqlKeyRepo, LibSqlOAuth2AuthorizationCodeRepo,
    LibSqlOAuth2UserConsentRepo, LibSqlUserRepo,
};
use idp_service::oauth2::OAuth2Service;
use libsql::Database;
use management_service::{
    ManagementService,
    libsql::{LibSqlPermissionRepo, LibSqlRoleRepo},
};

pub(crate) type ManagementRouterService =
    ManagementService<LibSqlApplicationRepo, LibSqlPermissionRepo, LibSqlRoleRepo>;

type LocalOAuth2Service = OAuth2Service<
    LibSqlApplicationRepo,
    LibSqlClientRepo,
    LibSqlOAuth2AuthorizationCodeRepo,
    LibSqlUserRepo,
    LibSqlOAuth2UserConsentRepo,
    LibSqlKeyRepo,
>;

#[derive(Clone)]
pub struct RouterState {
    pub(crate) api_base_uri: String,
    pub(crate) database: Arc<Database>,
    pub(crate) management_service: Arc<ManagementRouterService>,
    pub(crate) global_identity_read_gate: Arc<GlobalIdentityReadGateSlot>,
    pub(crate) oauth2_service: Arc<LocalOAuth2Service>,
}

impl RouterState {
    pub fn new(
        api_base_uri: impl Into<String>,
        database: Arc<Database>,
        global_identity_read_gate: Arc<GlobalIdentityReadGateSlot>,
        management_service: Arc<ManagementRouterService>,
        oauth2_service: Arc<LocalOAuth2Service>,
    ) -> Self {
        Self {
            api_base_uri: api_base_uri.into(),
            database,
            management_service,
            global_identity_read_gate,
            oauth2_service,
        }
    }
}
