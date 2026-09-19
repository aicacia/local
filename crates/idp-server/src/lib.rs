#![forbid(unsafe_code)]

#[cfg(feature = "cli")]
mod cli;
mod config;

mod device_identity;
mod router;

#[cfg(feature = "cli")]
pub use cli::run;
pub use config::{AppConfig, PairingConfig};

pub use device_identity::{delete as delete_device_identity, open as open_device_identity};
pub use router::{
    DeviceIdentity, PairingAcceptanceController, PairingAcceptanceControllerSlot, RouterState,
    TimedPairingAcceptanceController, authorize_bearer, openapi_router, storage_router,
};
