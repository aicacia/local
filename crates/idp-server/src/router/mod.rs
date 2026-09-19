mod middleware;
mod openapi;
mod openapi_router;
mod pairing_acceptance;
mod routes;
mod state;
mod storage;

pub use middleware::authorize_bearer;
pub use openapi_router::openapi_router;
pub use pairing_acceptance::{
    PairingAcceptanceController, PairingAcceptanceControllerSlot, TimedPairingAcceptanceController,
};
#[allow(unused_imports)]
pub use state::{
    DeviceIdentity, GlobalIdentityReadGateSlot, RouterState, SetupJoinExecutor, SetupNewExecutor,
};
pub use storage::storage_router;
