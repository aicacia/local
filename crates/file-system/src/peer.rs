use alloc::vec::Vec;

pub trait PeerCodec {
    type Error;
    type PeerId: Ord + Clone;

    fn encode(peer: &Self::PeerId) -> Vec<u8>;
    fn decode(bytes: &[u8]) -> Result<Self::PeerId, Self::Error>;
}
