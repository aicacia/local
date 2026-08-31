use std::sync::Arc;

use libsql::Database;
use lidp_service::{
    management::ManagementService,
    oauth2::OAuth2Service,
    repo::{
        LibSqlApplicationRepo, LibSqlClientRepo, LibSqlKeyRepo, LibSqlOAuth2AuthorizationCodeRepo,
        LibSqlOAuth2UserConsentRepo, LibSqlUserRepo,
    },
};

#[derive(Clone)]
pub struct RouterState {
    pub(crate) api_base_uri: String,
    pub(crate) database: Arc<Database>,
    pub(crate) management_service: Arc<ManagementService>,
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
    pub fn new(
        api_base_uri: impl Into<String>,
        database: Arc<Database>,
        management_service: Arc<ManagementService>,
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
    ) -> Self {
        Self {
            api_base_uri: api_base_uri.into(),
            database,
            management_service,
            oauth2_service,
        }
    }
}
