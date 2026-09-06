#![forbid(unsafe_code)]

mod codec;
mod scoped_transport;
mod transport;

pub use codec::{EndpointIdCodec, EndpointIdCodecError};
pub use scoped_transport::{
    ScopedIrohIncoming, ScopedIrohTransport, StaticTunnelAuthorization, TunnelAuthorizationProvider,
};
pub use transport::{IrohIncoming, IrohTransport};
