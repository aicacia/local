#![cfg(feature = "in-memory")]

use core::{convert::Infallible, pin::Pin};
use std::sync::Arc;
#[cfg(feature = "native")]
use std::{env, fs};

#[cfg(feature = "native")]
use file_system::NativeStorage;
use file_system::{
    FileEntry, FileSystem, InMemoryStorage, MemoryTransport, MemoryTransportMutator, PeerCodec,
};
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

type TestFileSystem = FileSystem<InMemoryStorage, TestPeer, MemoryTransport<TestPeer>>;
#[cfg(feature = "native")]
type NativeTestFileSystem = FileSystem<NativeStorage, TestPeer, MemoryTransport<TestPeer>>;

async fn file_system_pair(
    corrupt_left_responses: bool,
) -> (
    TestFileSystem,
    TestFileSystem,
    MemoryTransport<TestPeer>,
    MemoryTransport<TestPeer>,
) {
    let left_mutator: Option<MemoryTransportMutator> = corrupt_left_responses.then(|| {
        Arc::new(|data: &mut Vec<u8>| {
            if data.get(1) == Some(&2) {
                *data.last_mut().expect("blob response is not empty") ^= 1;
            }
        }) as MemoryTransportMutator
    });
    let (left_transport, right_transport) =
        MemoryTransport::pair_with_mutators(TestPeer(1), TestPeer(2), left_mutator, None);
    let left_control = left_transport.clone();
    let right_control = right_transport.clone();
    let left = FileSystem::new(InMemoryStorage::new(), TestPeer(1), left_transport)
        .await
        .unwrap();
    let right = FileSystem::new(InMemoryStorage::new(), TestPeer(2), right_transport)
        .await
        .unwrap();
    (left, right, left_control, right_control)
}

async fn entry(file_system: &TestFileSystem, path: &str) -> FileEntry<TestPeer> {
    for _ in 0..100 {
        if let Ok(entry) = file_system.entry(path).await {
            return entry;
        }
        tokio::task::yield_now().await;
    }
    panic!("file metadata did not synchronize")
}

async fn entry_with_size(
    file_system: &TestFileSystem,
    path: &str,
    size: u64,
) -> FileEntry<TestPeer> {
    for _ in 0..100 {
        if let Ok(entry) = file_system.entry(path).await
            && entry.size == size
        {
            return entry;
        }
        tokio::task::yield_now().await;
    }
    panic!("updated file metadata did not synchronize")
}

#[tokio::test]
async fn syncs_a_file_created_on_another_node() {
    let (left, right, _, _) = file_system_pair(false).await;

    left.write("notes/today.txt", b"hello").await.unwrap();

    let entry = entry(&right, "notes/today.txt").await;
    assert_eq!(entry.size, 5);
    assert_eq!(
        entry.providers.into_iter().collect::<Vec<_>>(),
        [TestPeer(1)]
    );
    assert!(!entry.local);
}

#[tokio::test]
async fn appends_content_and_syncs_the_updated_file() {
    let (left, right, _, _) = file_system_pair(false).await;

    left.write("notes/today.txt", b"hello").await.unwrap();
    entry(&right, "notes/today.txt").await;
    left.append("notes/today.txt", b" world").await.unwrap();

    let entry = entry_with_size(&right, "notes/today.txt", 11).await;
    assert_eq!(entry.size, 11);
    assert!(!entry.local);
    assert_eq!(
        right
            .start_read("notes/today.txt")
            .await
            .unwrap()
            .await
            .unwrap(),
        b"hello world"
    );
    assert!(!right.entry("notes/today.txt").await.unwrap().local);
}

#[tokio::test]
async fn streams_passthrough_content_in_verified_order() {
    let (left, right, _, _) = file_system_pair(false).await;

    left.write("notes/today.txt", b"abcdef").await.unwrap();
    entry(&right, "notes/today.txt").await;
    let mut stream = right.start_stream("notes/today.txt", 2).await.unwrap();

    assert_eq!(next(&mut stream).await, Some(Ok(b"ab".to_vec())));
    assert_eq!(next(&mut stream).await, Some(Ok(b"cd".to_vec())));
    assert_eq!(next(&mut stream).await, Some(Ok(b"ef".to_vec())));
    assert_eq!(next(&mut stream).await, None);
    assert!(!right.entry("notes/today.txt").await.unwrap().local);
}

#[tokio::test]
async fn rejects_a_corrupt_passthrough_chunk() {
    let (left, right, _, _) = file_system_pair(true).await;

    left.write("notes/today.txt", b"hello").await.unwrap();
    entry(&right, "notes/today.txt").await;
    let mut stream = right.start_stream("notes/today.txt", 2).await.unwrap();

    assert!(matches!(next(&mut stream).await, Some(Err(_))));
}

#[tokio::test]
async fn merges_files_created_offline_in_the_same_folder() {
    let (left, right, _, _) = file_system_pair(false).await;

    left.write("notes/left.txt", b"left").await.unwrap();
    right.write("notes/right.txt", b"right").await.unwrap();

    for _ in 0..100 {
        if left.list("notes").await.unwrap().len() == 2
            && right.list("notes").await.unwrap().len() == 2
        {
            break;
        }
        tokio::task::yield_now().await;
    }

    assert_eq!(left.list("notes").await.unwrap().len(), 2);
    assert_eq!(right.list("notes").await.unwrap().len(), 2);
    assert!(!left.entry("notes/right.txt").await.unwrap().local);
    assert!(!right.entry("notes/left.txt").await.unwrap().local);
}

#[tokio::test]
async fn syncs_changes_made_by_each_node_while_offline() {
    let (left, right, left_transport, right_transport) = file_system_pair(false).await;

    right_transport.set_online(false);
    right.write("notes/right.txt", b"right").await.unwrap();
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    assert!(left.entry("notes/right.txt").await.is_err());

    right_transport.set_online(true);
    assert!(right_transport.is_online());
    entry(&left, "notes/right.txt").await;

    left_transport.set_online(false);
    left.write("notes/left.txt", b"left").await.unwrap();
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    assert!(right.entry("notes/left.txt").await.is_err());

    left_transport.set_online(true);
    assert!(left_transport.is_online());
    entry(&right, "notes/left.txt").await;
}

#[cfg(feature = "native")]
#[tokio::test]
async fn persists_offline_metadata_across_a_native_restart() {
    let root = env::temp_dir().join(format!("file-system-restart-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (transport, _remote) = MemoryTransport::pair(TestPeer(1), TestPeer(2));
    transport.set_online(false);
    let file_system: NativeTestFileSystem =
        FileSystem::new(NativeStorage::new(&root).unwrap(), TestPeer(1), transport)
            .await
            .unwrap();
    file_system
        .write("notes/offline.txt", b"offline")
        .await
        .unwrap();
    drop(file_system);

    let (transport, _remote) = MemoryTransport::pair(TestPeer(1), TestPeer(2));
    let file_system: NativeTestFileSystem =
        FileSystem::new(NativeStorage::new(&root).unwrap(), TestPeer(1), transport)
            .await
            .unwrap();
    let entry = file_system.entry("notes/offline.txt").await.unwrap();
    assert_eq!(entry.size, 7);
    assert!(entry.local);
    assert_eq!(
        file_system.read("notes/offline.txt").await.unwrap(),
        b"offline"
    );
    drop(file_system);
    let _ = fs::remove_dir_all(root);
}

async fn next(
    stream: &mut file_system::ReadStream<file_system::Error>,
) -> Option<Result<Vec<u8>, file_system::ReadError<file_system::Error>>> {
    core::future::poll_fn(|context| Pin::new(&mut *stream).poll_next(context)).await
}
