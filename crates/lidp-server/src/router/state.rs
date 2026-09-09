use std::{future::Future, pin::Pin, sync::Arc};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use iroh::{Endpoint, EndpointId, SecretKey};
use libsql::Database;
use lidp_model::contract::{ErrorCode, ErrorResponse};
use lidp_service::{
    hosted_control_plane::HostedControlPlane,
    oauth2::OAuth2Service,
    repo::{
        LibSqlApplicationRepo, LibSqlClientRepo, LibSqlDeviceRepo, LibSqlKeyRepo,
        LibSqlOAuth2AuthorizationCodeRepo, LibSqlOAuth2UserConsentRepo, LibSqlUserRepo,
    },
    storage_session::StorageScope,
    storage_session::StorageSessionService,
};

#[derive(Clone)]
pub struct DeviceIdentity {
    endpoint: Endpoint,
    secret_key: SecretKey,
}

impl DeviceIdentity {
    #[must_use]
    pub fn new(endpoint: Endpoint, secret_key: SecretKey) -> Self {
        Self {
            endpoint,
            secret_key,
        }
    }

    #[must_use]
    pub fn endpoint_id(&self) -> EndpointId {
        self.endpoint.id()
    }

    #[must_use]
    pub fn endpoint(&self) -> Endpoint {
        self.endpoint.clone()
    }

    pub fn endpoint_address(&self) -> Result<String, String> {
        serde_json::to_string(&self.endpoint.addr()).map_err(|error| error.to_string())
    }

    #[must_use]
    pub fn sign(&self, message: &[u8]) -> String {
        URL_SAFE_NO_PAD.encode(self.secret_key.sign(message).to_bytes())
    }
}

pub trait StorageScopeResolver: Send + Sync + 'static {
    fn resolve(
        &self,
        bearer_token: String,
    ) -> Pin<Box<dyn Future<Output = Result<StorageScope, ErrorResponse>> + Send + '_>>;
}

pub struct HostedStorageScopeResolver {
    control_plane: Arc<HostedControlPlane>,
}

impl HostedStorageScopeResolver {
    #[must_use]
    pub fn new(control_plane: Arc<HostedControlPlane>) -> Self {
        Self { control_plane }
    }
}

impl StorageScopeResolver for HostedStorageScopeResolver {
    fn resolve(
        &self,
        bearer_token: String,
    ) -> Pin<Box<dyn Future<Output = Result<StorageScope, ErrorResponse>> + Send + '_>> {
        let control_plane = Arc::clone(&self.control_plane);
        Box::pin(async move {
            control_plane
                .storage_scope(&bearer_token)
                .await
                .map_err(|_| ErrorResponse::new(ErrorCode::NotAuthorized))
        })
    }
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
    pub devices: Arc<LibSqlDeviceRepo>,
    pub device_identity: Arc<DeviceIdentity>,
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
        devices: Arc<LibSqlDeviceRepo>,
        device_identity: Arc<DeviceIdentity>,
    ) -> Self {
        Self {
            ui_base_uri: ui_base_uri.into(),
            api_base_uri: api_base_uri.into(),
            database,
            oauth2_service,
            storage_sessions,
            devices,
            device_identity,
            storage_scope_resolver: None,
        }
    }

    pub fn with_storage_scope_resolver(mut self, resolver: Arc<dyn StorageScopeResolver>) -> Self {
        self.storage_scope_resolver = Some(resolver);
        self
    }
}
