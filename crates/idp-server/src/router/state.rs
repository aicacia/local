use std::{
    fs,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{Arc, Mutex},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use bootstrap_service::bootstrap::BootstrapInput;
use file_system::Residency;

use idp_service::{
    libsql::{
        LibSqlApplicationRepo, LibSqlClientRepo, LibSqlKeyRepo, LibSqlOAuth2AuthorizationCodeRepo,
        LibSqlOAuth2UserConsentRepo, LibSqlUserRepo,
    },
    oauth2::OAuth2Service,
};
use iroh::{Endpoint, EndpointId, SecretKey};
use libsql::Database;
use management_service::HostedControlPlane;
use management_service::libsql::LibSqlDeviceRepo;
use storage_service::ScopedFileSystemRuntime;

use crate::{
    GlobalIdentityReadGate,
    local_setup::{LocalSetup, LocalSetupJoin, LocalSetupState},
};

use super::PairingAcceptanceControllerSlot;

#[derive(Clone)]
pub(crate) struct LocalSetupServices {
    pub setup: Arc<LocalSetup>,
    pub residency_policy: Arc<Mutex<DeviceResidencyPolicy>>,
    pub residency_root: PathBuf,
}

pub(crate) struct DeviceResidencyPolicy {
    device_default: Residency,
}

impl DeviceResidencyPolicy {
    fn load(root: &std::path::Path) -> std::io::Result<Self> {
        match fs::read(root.join("storage-residency.json")) {
            Ok(content) => {
                let value: serde_json::Value = serde_json::from_slice(&content)
                    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
                Ok(Self {
                    device_default: match value
                        .get("device_default")
                        .and_then(serde_json::Value::as_str)
                    {
                        Some("full") => Residency::Full,
                        _ => Residency::Passthrough,
                    },
                })
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error),
        }
    }

    pub(crate) fn save(&self, root: &std::path::Path) -> std::io::Result<()> {
        let device_default = match self.device_default {
            Residency::Full => "full",
            Residency::Passthrough => "passthrough",
        };
        fs::write(
            root.join("storage-residency.json"),
            format!("{{\n  \"device_default\": \"{device_default}\"\n}}\n"),
        )
    }

    pub(crate) fn device_default(&self) -> Residency {
        self.device_default
    }

    pub(crate) fn set_device_default(&mut self, residency: Residency) {
        self.device_default = residency;
    }
}

impl Default for DeviceResidencyPolicy {
    fn default() -> Self {
        Self {
            device_default: Residency::Passthrough,
        }
    }
}

pub trait SetupNewExecutor: Send + Sync + 'static {
    fn bootstrap(
        &self,
        input: BootstrapInput,
        device: (String, String),
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>>;
}

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

pub trait SetupJoinExecutor: Send + Sync + 'static {
    fn join(
        &self,
        setup: Arc<LocalSetup>,
        join: LocalSetupJoin,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>>;
}

pub struct SetupNewExecutorSlot {
    executor: Mutex<Option<Arc<dyn SetupNewExecutor>>>,
}

impl SetupNewExecutorSlot {
    #[must_use]
    pub fn new() -> Self {
        Self {
            executor: Mutex::new(None),
        }
    }

    pub fn bind(&self, executor: Arc<dyn SetupNewExecutor>) -> Result<(), String> {
        let mut slot = self
            .executor
            .lock()
            .map_err(|_| "setup new executor lock is poisoned".to_owned())?;
        *slot = Some(executor);
        Ok(())
    }

    pub fn executor(&self) -> Result<Arc<dyn SetupNewExecutor>, String> {
        self.executor
            .lock()
            .map_err(|_| "setup new executor lock is poisoned".to_owned())?
            .clone()
            .ok_or_else(|| "setup new executor is not initialized".to_owned())
    }
}

impl Default for SetupNewExecutorSlot {
    fn default() -> Self {
        Self::new()
    }
}

pub struct GlobalIdentityReadGateSlot {
    gate: Mutex<Option<Arc<dyn GlobalIdentityReadGate>>>,
}

impl GlobalIdentityReadGateSlot {
    #[must_use]
    pub fn new() -> Self {
        Self {
            gate: Mutex::new(None),
        }
    }

    pub fn bind(&self, gate: Arc<dyn GlobalIdentityReadGate>) -> Result<(), String> {
        let mut slot = self
            .gate
            .lock()
            .map_err(|_| "global identity read gate lock is poisoned".to_owned())?;
        *slot = Some(gate);
        Ok(())
    }

    pub async fn verify(&self) -> bool {
        let Ok(gate) = self.gate.lock().map(|slot| slot.clone()) else {
            return false;
        };
        match gate {
            Some(gate) => gate.verify().await,
            None => false,
        }
    }
}

impl Default for GlobalIdentityReadGateSlot {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SetupJoinExecutorSlot {
    executor: Mutex<Option<Arc<dyn SetupJoinExecutor>>>,
}

impl SetupJoinExecutorSlot {
    #[must_use]
    pub fn new() -> Self {
        Self {
            executor: Mutex::new(None),
        }
    }

    pub fn bind(&self, executor: Arc<dyn SetupJoinExecutor>) -> Result<(), String> {
        let mut slot = self
            .executor
            .lock()
            .map_err(|_| "setup join executor lock is poisoned".to_owned())?;
        *slot = Some(executor);
        Ok(())
    }

    pub fn executor(&self) -> Result<Arc<dyn SetupJoinExecutor>, String> {
        self.executor
            .lock()
            .map_err(|_| "setup join executor lock is poisoned".to_owned())?
            .clone()
            .ok_or_else(|| "setup join executor is not initialized".to_owned())
    }
}

impl Default for SetupJoinExecutorSlot {
    fn default() -> Self {
        Self::new()
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

    pub devices: Arc<LibSqlDeviceRepo>,
    pub device_identity: Arc<DeviceIdentity>,
    pub pairing_acceptance: Arc<PairingAcceptanceControllerSlot>,
    pub hosted_control_plane: Option<Arc<HostedControlPlane>>,

    pub storage_file_systems: Option<Arc<ScopedFileSystemRuntime<EndpointId>>>,
    pub(crate) local_setup: Option<LocalSetupServices>,
    pub setup_join_executor: Arc<SetupJoinExecutorSlot>,
    pub setup_new_executor: Arc<SetupNewExecutorSlot>,
    pub global_identity_read_gate: Arc<GlobalIdentityReadGateSlot>,
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

        devices: Arc<LibSqlDeviceRepo>,
        device_identity: Arc<DeviceIdentity>,
    ) -> Self {
        Self {
            ui_base_uri: ui_base_uri.into(),
            api_base_uri: api_base_uri.into(),
            database,
            oauth2_service,

            devices,
            device_identity,
            pairing_acceptance: Arc::new(PairingAcceptanceControllerSlot::new()),
            hosted_control_plane: None,

            storage_file_systems: None,
            local_setup: None,
            setup_join_executor: Arc::new(SetupJoinExecutorSlot::new()),
            setup_new_executor: Arc::new(SetupNewExecutorSlot::new()),
            global_identity_read_gate: Arc::new(GlobalIdentityReadGateSlot::new()),
        }
    }

    pub fn with_local_setup(
        mut self,
        data_dir: impl Into<PathBuf>,
        setup_state: LocalSetupState,
    ) -> Self {
        let data_dir = data_dir.into();
        let residency_policy = DeviceResidencyPolicy::load(&data_dir)
            .expect("failed to load local storage residency policy");
        self.local_setup = Some(LocalSetupServices {
            setup: Arc::new(LocalSetup::new(data_dir.clone(), setup_state)),
            residency_policy: Arc::new(Mutex::new(residency_policy)),
            residency_root: data_dir,
        });
        self
    }

    pub fn with_setup_join_executor(self, executor: Arc<dyn SetupJoinExecutor>) -> Self {
        self.setup_join_executor
            .bind(executor)
            .expect("setup join executor is initialized once");
        self
    }

    pub fn with_setup_new_executor(self, executor: Arc<dyn SetupNewExecutor>) -> Self {
        self.setup_new_executor
            .bind(executor)
            .expect("setup new executor is initialized once");
        self
    }

    pub fn setup_token(&self) -> Option<String> {
        self.local_setup.as_ref()?.setup.token()
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

    pub fn with_global_identity_read_gate(self, gate: Arc<dyn GlobalIdentityReadGate>) -> Self {
        self.global_identity_read_gate
            .bind(gate)
            .expect("global identity read gate is initialized once");
        self
    }

    pub async fn global_identity_is_current(&self) -> bool {
        self.global_identity_read_gate.verify().await
    }
}
