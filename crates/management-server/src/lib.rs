#![forbid(unsafe_code)]

#[cfg(feature = "cli")]
mod cli;
mod config;
mod router;

#[cfg(feature = "cli")]
pub use cli::run;
pub use config::AppConfig;
pub use router::{RouterState, openapi_router};
