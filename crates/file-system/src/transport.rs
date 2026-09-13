use alloc::vec::Vec;
use core::future::Future;

use futures_core::Stream;

pub trait Transport {
    type Error;
    type PeerId;
    type Incoming: Stream<Item = (Self::PeerId, Vec<u8>)> + Send;

    fn send(
        &self,
        peer: Self::PeerId,
        data: Vec<u8>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn broadcast(&self, data: Vec<u8>) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn peers(&self) -> Vec<Self::PeerId>;
    fn subscribe(&self) -> impl Future<Output = Result<Self::Incoming, Self::Error>> + Send;
}
