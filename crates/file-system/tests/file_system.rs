#![cfg(feature = "in-memory")]

use core::{
    convert::Infallible,
    future::{Future, ready},
    pin::Pin,
    task::{Context, Poll},
};

use file_system::{FileSystem, InMemoryStorage, Transport};
use futures_core::Stream;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct TestPeer(u16);

struct TestTransport;

struct NoIncoming;

impl Stream for NoIncoming {
    type Item = (TestPeer, Vec<u8>);

    fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(None)
    }
}

impl Transport for TestTransport {
    type Error = Infallible;
    type PeerId = TestPeer;
    type Incoming = NoIncoming;

    fn send(
        &self,
        _: TestPeer,
        _: Vec<u8>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(Ok(()))
    }

    fn broadcast(&self, _: Vec<u8>) -> impl Future<Output = Result<(), Self::Error>> + Send {
        ready(Ok(()))
    }

    fn subscribe(&self) -> impl Future<Output = Result<Self::Incoming, Self::Error>> + Send {
        ready(Ok(NoIncoming))
    }
}

#[test]
fn file_system_binds_storage_to_its_transport_peer_type() {
    let storage = InMemoryStorage::new(TestPeer(7));
    let mut file_system = FileSystem::new(storage, TestTransport);

    let entry = file_system.write("notes/today.txt", b"hello").unwrap();

    assert_eq!(
        entry.providers.into_iter().collect::<Vec<_>>(),
        [TestPeer(7)]
    );
    assert_eq!(file_system.read("notes/today.txt").unwrap(), b"hello");
    assert_eq!(
        file_system.storage().entry("notes/today.txt").unwrap().hash,
        entry.hash
    );
}
