use core::fmt;

use file_system::PeerCodec;
use iroh::EndpointId;

pub struct EndpointIdCodec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EndpointIdCodecError;

impl fmt::Display for EndpointIdCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid Iroh endpoint ID")
    }
}

impl std::error::Error for EndpointIdCodecError {}

impl PeerCodec for EndpointIdCodec {
    type Error = EndpointIdCodecError;
    type PeerId = EndpointId;

    fn encode(peer: &Self::PeerId) -> Vec<u8> {
        peer.as_bytes().to_vec()
    }

    fn decode(bytes: &[u8]) -> Result<Self::PeerId, Self::Error> {
        let bytes: &[u8; 32] = bytes.try_into().map_err(|_| EndpointIdCodecError)?;
        EndpointId::from_bytes(bytes).map_err(|_| EndpointIdCodecError)
    }
}
