#![forbid(unsafe_code)]

mod scoped_transport;

pub use scoped_transport::{
    ScopedIrohTransport, StaticTunnelAuthorization, TunnelAuthorizationProvider,
};
