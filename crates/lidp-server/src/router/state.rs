use std::sync::Arc;

use libsql::Database;
use lidp_service::{
    oauth2::OAuth2Service,
    repo::{
        LibSqlApplicationRepo, LibSqlClientRepo, LibSqlKeyRepo, LibSqlOAuth2AuthorizationCodeRepo,
        LibSqlOAuth2UserConsentRepo, LibSqlUserRepo,
    },
};

#[derive(Clone)]
pub struct RouterState {
    pub ui_base_uri: String,
    pub api_base_uri: String,
    pub database: Arc<Database>,
    pub oauth2_service: Arc<
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
        ui_base_uri: impl Into<String>,
        api_base_uri: impl Into<String>,
        database: Arc<Database>,
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
            ui_base_uri: ui_base_uri.into(),
            api_base_uri: api_base_uri.into(),
            database,
            oauth2_service,
        }
    }
}
