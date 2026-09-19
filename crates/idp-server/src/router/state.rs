use std::sync::Arc;

use idp_service::{
    oauth2::OAuth2Service,
    replica::{
        DbApplicationRepo, DbClientRepo, DbKeyRepo, DbOAuth2AuthorizationCodeRepo,
        DbOAuth2UserConsentRepo, DbUserRepo,
    },
};
use iroh::{Endpoint, EndpointId, SecretKey};
use management_service::{HostedControlPlane, replica::DbDeviceRepo};
use storage_service::ScopedFileSystemRuntime;
use sync_db::{AutomergeRowCodec, RedbKernel};

use super::PairingAcceptanceControllerSlot;

type NativeOAuth2Service = OAuth2Service<
    DbApplicationRepo<RedbKernel, AutomergeRowCodec>,
    DbClientRepo<RedbKernel, AutomergeRowCodec>,
    DbOAuth2AuthorizationCodeRepo<RedbKernel, AutomergeRowCodec>,
    DbUserRepo<RedbKernel, AutomergeRowCodec>,
    DbOAuth2UserConsentRepo<RedbKernel, AutomergeRowCodec>,
    DbKeyRepo<RedbKernel, AutomergeRowCodec>,
>;

pub type NativeDeviceRepo = DbDeviceRepo<RedbKernel, AutomergeRowCodec>;
pub type NativeOAuth2ServiceRef = NativeOAuth2Service;

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
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

        URL_SAFE_NO_PAD.encode(self.secret_key.sign(message).to_bytes())
    }
}

#[derive(Clone)]
pub struct RouterState {
    pub ui_base_uri: String,
    pub api_base_uri: String,
    pub engine: Arc<db::NativeEngine>,
    pub oauth2_service: Arc<NativeOAuth2Service>,
    pub devices: Arc<NativeDeviceRepo>,
    pub device_identity: Arc<DeviceIdentity>,
    pub pairing_acceptance: Arc<PairingAcceptanceControllerSlot>,
    pub hosted_control_plane: Option<Arc<HostedControlPlane>>,
    pub storage_file_systems: Option<Arc<ScopedFileSystemRuntime<EndpointId>>>,
}

impl RouterState {
    pub fn new(
        ui_base_uri: impl Into<String>,
        api_base_uri: impl Into<String>,
        engine: Arc<db::NativeEngine>,
        oauth2_service: Arc<NativeOAuth2Service>,
        devices: Arc<NativeDeviceRepo>,
        device_identity: Arc<DeviceIdentity>,
    ) -> Self {
        Self {
            ui_base_uri: ui_base_uri.into(),
            api_base_uri: api_base_uri.into(),
            engine,
            oauth2_service,
            devices,
            device_identity,
            pairing_acceptance: Arc::new(PairingAcceptanceControllerSlot::new()),
            hosted_control_plane: None,
            storage_file_systems: None,
        }
    }

    pub fn with_hosted_control_plane(mut self, control_plane: Arc<HostedControlPlane>) -> Self {
        self.hosted_control_plane = Some(control_plane);
        self
    }

    pub fn with_storage_file_systems(
        mut self,
        file_systems: Arc<ScopedFileSystemRuntime<EndpointId>>,
    ) -> Self {
        self.storage_file_systems = Some(file_systems);
        self
    }
}
