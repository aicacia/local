#![forbid(unsafe_code)]

mod dynamic;
#[cfg(feature = "in-memory")]
mod in_memory;
mod peer;
mod server;
mod store;
mod tunnel;

pub use dynamic::DynamicEndpointIdStore;
#[cfg(feature = "in-memory")]
pub use in_memory::InMemoryEndpointIdStore;
pub use peer::{Peer, PeerReader, PeerWriter};
pub use server::{IRON_CHAIN_V1_ALPN, Server, ServerEvent};
pub use store::AllowedEndpointId;
pub use tunnel::{
    TUNNEL_ALPN, Tunnel, TunnelAuthorizer, TunnelEvent, TunnelManager, TunnelReader, TunnelWriter,
    VaultId,
};
