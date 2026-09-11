#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

#[cfg(not(feature = "std"))]
extern crate alloc;

pub mod bootstrap;
pub mod device_enrollment;
#[cfg(feature = "std")]
pub mod hosted_control_plane;
pub mod management;
pub mod oauth2;
mod password_config;
pub mod repo;

#[cfg(feature = "std")]
pub mod storage_session;
#[cfg(feature = "std")]
pub mod tunnel_authorization;
mod util;

pub use password_config::PasswordConfig;
