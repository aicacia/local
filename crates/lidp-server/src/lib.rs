#![forbid(unsafe_code)]

#[cfg(feature = "cli")]
mod cli;
mod config;
mod router;

#[cfg(feature = "cli")]
pub use cli::run;
pub use config::AppConfig;
pub use router::{
    DeviceIdentity, HostedStorageScopeResolver, RouterState, StorageScopeResolver,
    authorize_bearer, openapi_router, storage_router, storage_session_openapi_router,
};
