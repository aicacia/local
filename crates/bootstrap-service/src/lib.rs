#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

#[cfg(not(feature = "std"))]
extern crate alloc;

pub mod bootstrap;
mod error;

pub use error::{BootstrapError, BootstrapResult};
