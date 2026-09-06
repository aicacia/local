#![cfg(feature = "in-memory")]

use core::{
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use file_system::{FileSystem, InMemoryStorage, PeerCodec, SyncRequest};
use futures_core::Stream;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct TestPeer(u8);

impl PeerCodec for TestPeer {
    type Error = Infallible;
    type PeerId = Self;

    fn encode(peer: &Self::PeerId) -> Vec<u8> {
        vec![peer.0]
    }

    fn decode(bytes: &[u8]) -> Result<Self::PeerId, Self::Error> {
        Ok(Self(bytes[0]))
    }
}

#[test]
fn syncs_a_file_created_on_another_node() {
    let mut left = FileSystem::new(InMemoryStorage::new(TestPeer(1)));
    let mut right = FileSystem::new(InMemoryStorage::new(TestPeer(2)));
    let mut left_requests = left.take_sync_requests().unwrap();
    let mut right_requests = right.take_sync_requests().unwrap();

    left.write("notes/today.txt", b"hello").unwrap();
    exchange(
        &mut left,
        &mut right,
        &mut left_requests,
        &mut right_requests,
    );

    let entry = right.entry("notes/today.txt").unwrap();
    assert_eq!(entry.size, 5);
    assert_eq!(
        entry.providers.into_iter().collect::<Vec<_>>(),
        [TestPeer(1)]
    );
    assert!(!entry.local);
}

#[test]
fn reads_passthrough_content_from_a_provider() {
    let mut left = FileSystem::new(InMemoryStorage::new(TestPeer(1)));
    let mut right = FileSystem::new(InMemoryStorage::new(TestPeer(2)));
    let mut left_requests = left.take_sync_requests().unwrap();
    let mut right_requests = right.take_sync_requests().unwrap();

    left.write("notes/today.txt", b"hello").unwrap();
    exchange(
        &mut left,
        &mut right,
        &mut left_requests,
        &mut right_requests,
    );
    let mut read = right.start_read("notes/today.txt").unwrap();
    exchange(
        &mut left,
        &mut right,
        &mut left_requests,
        &mut right_requests,
    );

    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert_eq!(
        Pin::new(&mut read).poll(&mut context),
        Poll::Ready(Ok(b"hello".to_vec()))
    );
    assert!(!right.entry("notes/today.txt").unwrap().local);
}

#[test]
fn streams_passthrough_content_in_verified_order() {
    let mut left = FileSystem::new(InMemoryStorage::new(TestPeer(1)));
    let mut right = FileSystem::new(InMemoryStorage::new(TestPeer(2)));
    let mut left_requests = left.take_sync_requests().unwrap();
    let mut right_requests = right.take_sync_requests().unwrap();

    left.write("notes/today.txt", b"abcdef").unwrap();
    exchange(
        &mut left,
        &mut right,
        &mut left_requests,
        &mut right_requests,
    );
    let mut stream = right.start_stream("notes/today.txt", 2).unwrap();
    exchange(
        &mut left,
        &mut right,
        &mut left_requests,
        &mut right_requests,
    );

    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert_eq!(
        Pin::new(&mut stream).poll_next(&mut context),
        Poll::Ready(Some(Ok(b"ab".to_vec())))
    );
    assert_eq!(
        Pin::new(&mut stream).poll_next(&mut context),
        Poll::Ready(Some(Ok(b"cd".to_vec())))
    );
    assert_eq!(
        Pin::new(&mut stream).poll_next(&mut context),
        Poll::Ready(Some(Ok(b"ef".to_vec())))
    );
    assert_eq!(
        Pin::new(&mut stream).poll_next(&mut context),
        Poll::Ready(None)
    );
    assert!(!right.entry("notes/today.txt").unwrap().local);
}

#[test]
fn rejects_a_corrupt_passthrough_chunk() {
    let mut left = FileSystem::new(InMemoryStorage::new(TestPeer(1)));
    let mut right = FileSystem::new(InMemoryStorage::new(TestPeer(2)));
    let mut left_requests = left.take_sync_requests().unwrap();
    let mut right_requests = right.take_sync_requests().unwrap();

    left.write("notes/today.txt", b"hello").unwrap();
    exchange(
        &mut left,
        &mut right,
        &mut left_requests,
        &mut right_requests,
    );
    let mut stream = right.start_stream("notes/today.txt", 2).unwrap();
    let SyncRequest::Send { data, .. } = right_requests.try_recv().unwrap() else {
        panic!("expected a content request");
    };
    left.receive(TestPeer(2), data).unwrap();
    let SyncRequest::Send { mut data, .. } = left_requests.try_recv().unwrap() else {
        panic!("expected a content response");
    };
    *data.last_mut().unwrap() ^= 1;
    right.receive(TestPeer(1), data).unwrap();

    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    assert!(matches!(
        Pin::new(&mut stream).poll_next(&mut context),
        Poll::Ready(Some(Err(_)))
    ));
}

#[test]
fn merges_files_created_offline_in_the_same_folder() {
    let mut left = FileSystem::new(InMemoryStorage::new(TestPeer(1)));
    let mut right = FileSystem::new(InMemoryStorage::new(TestPeer(2)));
    let mut left_requests = left.take_sync_requests().unwrap();
    let mut right_requests = right.take_sync_requests().unwrap();

    left.write("notes/left.txt", b"left").unwrap();
    right.write("notes/right.txt", b"right").unwrap();
    exchange(
        &mut left,
        &mut right,
        &mut left_requests,
        &mut right_requests,
    );

    assert_eq!(left.list("notes").unwrap().len(), 2);
    assert_eq!(right.list("notes").unwrap().len(), 2);
    assert!(!left.entry("notes/right.txt").unwrap().local);
    assert!(!right.entry("notes/left.txt").unwrap().local);
}

fn exchange(
    left: &mut FileSystem<InMemoryStorage<TestPeer>>,
    right: &mut FileSystem<InMemoryStorage<TestPeer>>,
    left_requests: &mut tokio::sync::mpsc::Receiver<SyncRequest<TestPeer>>,
    right_requests: &mut tokio::sync::mpsc::Receiver<SyncRequest<TestPeer>>,
) {
    for _ in 0..8 {
        while let Ok(request) = left_requests.try_recv() {
            match request {
                SyncRequest::Broadcast(data) => right.receive(TestPeer(1), data).unwrap(),
                SyncRequest::Send { peer, data } => {
                    assert_eq!(peer, TestPeer(2));
                    right.receive(TestPeer(1), data).unwrap();
                }
            }
        }
        while let Ok(request) = right_requests.try_recv() {
            match request {
                SyncRequest::Broadcast(data) => left.receive(TestPeer(2), data).unwrap(),
                SyncRequest::Send { peer, data } => {
                    assert_eq!(peer, TestPeer(1));
                    left.receive(TestPeer(2), data).unwrap();
                }
            }
        }
    }
}
