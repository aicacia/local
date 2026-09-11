#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

#[cfg(not(feature = "std"))]
extern crate alloc;

extern crate self as idp_service;

#[cfg(feature = "fs")]
pub mod fs;
#[cfg(feature = "libsql")]
pub mod libsql;
pub mod oauth2;
mod password_config;
pub mod repo;

mod util;

pub use password_config::PasswordConfig;
pub use util::{encrypt_password, generate_random_string};
