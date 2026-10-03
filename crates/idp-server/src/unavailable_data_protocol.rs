use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};

#[derive(Clone, Copy, Debug)]
pub(crate) struct UnavailableDataProtocol;

impl ProtocolHandler for UnavailableDataProtocol {
    async fn accept(&self, _connection: Connection) -> Result<(), AcceptError> {
        Ok(())
    }
}
