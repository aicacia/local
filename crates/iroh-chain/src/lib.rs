#![forbid(unsafe_code)]

mod dynamic;
#[cfg(feature = "in-memory")]
mod in_memory;
mod server;
mod store;

pub use dynamic::DynamicEndpointIdStore;
#[cfg(feature = "in-memory")]
pub use in_memory::InMemoryEndpointIdStore;
pub use server::{
    Server, TUNNEL_ALPN, Tunnel, TunnelAuthorizer, TunnelEvent, TunnelReader, TunnelWriter, VaultId,
};
pub use store::AllowedEndpointId;
