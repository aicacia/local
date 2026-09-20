#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(feature = "fs")]
pub mod fs;

#[cfg(feature = "replica")]
pub mod replica;

#[cfg(feature = "std")]
pub mod access_token_authorization;
#[cfg(feature = "std")]
mod device_enrollment;
mod device_repo;
mod error;
#[cfg(feature = "std")]
mod hosted_control_plane;
mod permission_repo;
mod role_repo;
mod service;
#[cfg(feature = "std")]
mod storage_session;

#[cfg(feature = "std")]
pub use device_enrollment::DeviceEnrollmentService;
pub use device_repo::DeviceRepo;
pub use error::{ManagementError, ManagementResult};
#[cfg(feature = "std")]
pub use hosted_control_plane::HostedControlPlane;
pub use permission_repo::PermissionRepo;
pub use role_repo::RoleRepo;
pub use service::{MANAGEMENT_APPLICATION_URI, ManagementService};
#[cfg(feature = "std")]
pub use storage_session::{StorageScope, StorageSessionService};
