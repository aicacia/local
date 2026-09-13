#![forbid(unsafe_code)]

#[cfg(feature = "cli")]
mod cli;
mod config;
mod device_identity;
mod global_bootstrap_grants;
mod global_identity_cache;
mod global_identity_join;
mod global_identity_read_gate;
mod global_identity_revision_writer;
mod local_setup;
mod router;

#[cfg(feature = "cli")]
pub use cli::run;
pub use config::{AppConfig, PairingConfig};
pub use device_identity::{delete as delete_device_identity, open as open_device_identity};
pub use global_bootstrap_grants::{GlobalBootstrapGrants, GlobalBootstrapTunnelAuthorizer};
pub use global_identity_cache::GlobalIdentityCache;
pub use global_identity_join::GlobalIdentityJoinApprover;
pub use global_identity_read_gate::{ActiveGlobalIdentityReadGate, GlobalIdentityReadGate};
pub use global_identity_revision_writer::GlobalIdentityRevisionWriter;
pub use local_setup::{LocalSetup, LocalSetupJoin, LocalSetupState, SetupStage};

pub use router::{
    DeviceIdentity, GlobalIdentityReadGateSlot, HostedStorageScopeResolver,
    PairingAcceptanceController, PairingAcceptanceControllerSlot, RouterState, SetupJoinExecutor,
    SetupNewExecutor, StorageScopeResolver, TimedPairingAcceptanceController, authorize_bearer,
    openapi_router, storage_router, storage_session_openapi_router,
};
