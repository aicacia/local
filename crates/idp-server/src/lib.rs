#![forbid(unsafe_code)]

#[cfg(feature = "cli")]
mod cli;
mod config;

mod bootstrap;
#[cfg(feature = "cli")]
mod data_protocol;
#[cfg(feature = "cli")]
mod database_protocol;
mod device_identity;
mod router;
#[cfg(feature = "cli")]
mod storage_protocol;

#[cfg(feature = "cli")]
pub use cli::run;
pub use config::{AppConfig, PairingConfig};

pub use device_identity::{
    delete as delete_device_identity, open as open_device_identity,
    open_with_allowlist as open_device_identity_with_allowlist,
};
pub use router::{
    DeviceIdentity, PairingAcceptanceController, PairingAcceptanceControllerSlot, RouterState,
    TimedPairingAcceptanceController, authorize_bearer, openapi_router, storage_router,
};
