use std::{future::Future, pin::Pin, sync::Arc};

use libsql::Database;
use lidp_model::contract::ErrorResponse;
use lidp_service::{
    oauth2::OAuth2Service,
    repo::{
        LibSqlApplicationRepo, LibSqlClientRepo, LibSqlKeyRepo, LibSqlOAuth2AuthorizationCodeRepo,
        LibSqlOAuth2UserConsentRepo, LibSqlUserDeviceRepo, LibSqlUserRepo,
    },
    storage_session::StorageScope,
    storage_session::StorageSessionService,
};

pub trait StorageScopeResolver: Send + Sync + 'static {
    fn resolve(
        &self,
        bearer_token: String,
    ) -> Pin<Box<dyn Future<Output = Result<StorageScope, ErrorResponse>> + Send + '_>>;
}

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
    pub storage_sessions: Arc<StorageSessionService>,
    pub user_devices: Arc<LibSqlUserDeviceRepo>,
    pub storage_scope_resolver: Option<Arc<dyn StorageScopeResolver>>,
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
        storage_sessions: Arc<StorageSessionService>,
        user_devices: Arc<LibSqlUserDeviceRepo>,
    ) -> Self {
        Self {
            ui_base_uri: ui_base_uri.into(),
            api_base_uri: api_base_uri.into(),
            database,
            oauth2_service,
            storage_sessions,
            user_devices,
            storage_scope_resolver: None,
        }
    }

    pub fn with_storage_scope_resolver(mut self, resolver: Arc<dyn StorageScopeResolver>) -> Self {
        self.storage_scope_resolver = Some(resolver);
        self
    }
}
