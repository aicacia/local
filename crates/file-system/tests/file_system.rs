#![cfg(feature = "in-memory")]

use core::convert::Infallible;

use file_system::{FileSystem, InMemoryStorage, PeerCodec, SyncRequest};

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
                SyncRequest::Send { peer, data } => right.receive(peer, data).unwrap(),
            }
        }
        while let Ok(request) = right_requests.try_recv() {
            match request {
                SyncRequest::Broadcast(data) => left.receive(TestPeer(2), data).unwrap(),
                SyncRequest::Send { peer, data } => left.receive(peer, data).unwrap(),
            }
        }
    }
}
