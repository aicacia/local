#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

#[cfg(not(feature = "std"))]
extern crate alloc;

pub mod contract;
#[cfg(feature = "migrate")]
pub mod migrate;
pub mod model;
