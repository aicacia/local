#![cfg(feature = "in-memory")]

use std::{convert::Infallible, pin::Pin, sync::Arc};
#[cfg(feature = "native")]
use std::{
    env, fs,
    sync::atomic::{AtomicUsize, Ordering},
};

#[cfg(feature = "native")]
use file_system::ContentHash;
#[cfg(feature = "native")]
use file_system::NativeStorage;
use file_system::{
    FileEntry, FileSystem, FileSystemError, InMemoryStorage, LocalFileSystem, MemoryTransport,
    MemoryTransportMutator, PeerCodec,
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

#[cfg(feature = "native")]
static NEXT_TEMP_DIR: AtomicUsize = AtomicUsize::new(0);

#[cfg(feature = "native")]
fn temp_dir(name: &str) -> std::path::PathBuf {
    env::temp_dir().join(format!(
        "file-system-{name}-{}-{}",
        std::process::id(),
        NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed)
    ))
}

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
async fn renames_a_file_and_syncs_the_new_path() {
    let (left, right, _, _) = file_system_pair(false).await;

    left.write("notes/today.txt", b"hello").await.unwrap();
    entry(&right, "notes/today.txt").await;

    left.rename("notes/today.txt", "archive/yesterday.txt")
        .await
        .unwrap();

    assert_eq!(left.read("archive/yesterday.txt").await.unwrap(), b"hello");
    assert!(matches!(
        left.entry("notes/today.txt").await,
        Err(FileSystemError::NotFound)
    ));
    entry(&right, "archive/yesterday.txt").await;
    assert!(matches!(
        right.entry("notes/today.txt").await,
        Err(FileSystemError::NotFound)
    ));
    assert_eq!(
        right
            .start_read("archive/yesterday.txt")
            .await
            .unwrap()
            .await,
        Ok(b"hello".to_vec())
    );
}

#[tokio::test]
async fn rejects_paths_reserved_for_internal_storage() {
    let (file_system, _, _, _) = file_system_pair(false).await;

    for path in [
        ".blobs/file",
        ".paths/file",
        ".passthrough/file",
        ".sync/file",
    ] {
        assert!(matches!(
            file_system.write(path, b"content").await,
            Err(FileSystemError::InvalidPath)
        ));
    }
}

#[tokio::test]
async fn opens_and_operates_without_a_transport() {
    let file_system: LocalFileSystem<InMemoryStorage> =
        LocalFileSystem::open_local(InMemoryStorage::new())
            .await
            .unwrap();

    file_system
        .write("notes/today.txt", b"hello")
        .await
        .unwrap();
    assert_eq!(file_system.read("notes/today.txt").await.unwrap(), b"hello");
    file_system
        .rename("notes/today.txt", "notes/archive.txt")
        .await
        .unwrap();
    file_system.delete("notes/archive.txt").await.unwrap();
    assert!(matches!(
        file_system.entry("notes/archive.txt").await,
        Err(FileSystemError::NotFound)
    ));
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
async fn materializes_and_evicts_remote_content() {
    let (left, right, _, _) = file_system_pair(false).await;

    left.write("notes/today.txt", b"hello").await.unwrap();
    entry(&right, "notes/today.txt").await;
    right.materialize("notes/today.txt").await.unwrap();
    assert!(right.entry("notes/today.txt").await.unwrap().local);
    assert_eq!(right.read("notes/today.txt").await.unwrap(), b"hello");

    right.evict("notes/today.txt").await.unwrap();
    assert!(!right.entry("notes/today.txt").await.unwrap().local);
    assert_eq!(
        right
            .start_read("notes/today.txt")
            .await
            .unwrap()
            .await
            .unwrap(),
        b"hello"
    );
}

#[tokio::test]
async fn refuses_to_evict_the_final_provider() {
    let (left, _right, _, _) = file_system_pair(false).await;

    left.write("notes/today.txt", b"hello").await.unwrap();
    assert!(matches!(
        left.evict("notes/today.txt").await,
        Err(file_system::FileSystemError::NoProvider)
    ));
    assert!(left.entry("notes/today.txt").await.unwrap().local);
}

#[tokio::test]
async fn passthrough_write_without_a_connected_peer_publishes_no_metadata() {
    let (left, right, _, right_transport) = file_system_pair(false).await;
    right_transport.set_online(false).await;

    assert!(matches!(
        left.write_passthrough_to_peer_or_any("notes/today.txt", b"hello")
            .await,
        Err(FileSystemError::NoProvider)
    ));
    assert!(left.entry("notes/today.txt").await.is_err());
    assert!(right.entry("notes/today.txt").await.is_err());
}

#[tokio::test]
async fn rejects_passthrough_upload_when_receiver_denies_the_path() {
    let (left, right, _, _) = file_system_pair(false).await;
    left.set_passthrough_admission(|_| false).await;

    assert!(
        right
            .write_passthrough(TestPeer(1), "notes/today.txt", b"hello")
            .await
            .is_err()
    );
    assert!(left.entry("notes/today.txt").await.is_err());
    assert!(right.entry("notes/today.txt").await.is_err());
}

#[tokio::test]
async fn uploads_passthrough_content_before_publishing_metadata() {
    let (left, right, _, _) = file_system_pair(false).await;

    let upload_entry = right
        .write_passthrough(TestPeer(1), "notes/today.txt", b"hello")
        .await
        .unwrap();

    assert_eq!(
        upload_entry.providers.into_iter().collect::<Vec<_>>(),
        [TestPeer(1)]
    );
    assert!(!upload_entry.local);
    assert!(entry(&left, "notes/today.txt").await.local);
    assert_eq!(left.read("notes/today.txt").await.unwrap(), b"hello");
    assert!(right.read("notes/today.txt").await.is_err());
}

#[tokio::test]
async fn rejects_corrupt_passthrough_upload_without_publishing_metadata() {
    let right_mutator: MemoryTransportMutator = Arc::new(|data: &mut Vec<u8>| {
        if data.get(1) == Some(&3) {
            *data.last_mut().expect("blob upload is not empty") ^= 1;
        }
    });
    let (left_transport, right_transport) =
        MemoryTransport::pair_with_mutators(TestPeer(1), TestPeer(2), None, Some(right_mutator));
    let left: TestFileSystem = FileSystem::new(InMemoryStorage::new(), TestPeer(1), left_transport)
        .await
        .unwrap();
    let right: TestFileSystem =
        FileSystem::new(InMemoryStorage::new(), TestPeer(2), right_transport)
            .await
            .unwrap();

    assert!(
        right
            .write_passthrough(TestPeer(1), "notes/today.txt", b"hello")
            .await
            .is_err()
    );
    assert!(right.entry("notes/today.txt").await.is_err());
    assert!(left.entry("notes/today.txt").await.is_err());
}

#[tokio::test]
async fn rejects_size_mismatched_passthrough_upload_without_publishing_metadata() {
    let right_mutator: MemoryTransportMutator = Arc::new(|data: &mut Vec<u8>| {
        if data.get(1) == Some(&3) {
            data[49] ^= 1;
        }
    });
    let (left_transport, right_transport) =
        MemoryTransport::pair_with_mutators(TestPeer(1), TestPeer(2), None, Some(right_mutator));
    let left: TestFileSystem = FileSystem::new(InMemoryStorage::new(), TestPeer(1), left_transport)
        .await
        .unwrap();
    let right: TestFileSystem =
        FileSystem::new(InMemoryStorage::new(), TestPeer(2), right_transport)
            .await
            .unwrap();

    assert!(
        right
            .write_passthrough(TestPeer(1), "notes/today.txt", b"hello")
            .await
            .is_err()
    );
    assert!(right.entry("notes/today.txt").await.is_err());
    assert!(left.entry("notes/today.txt").await.is_err());
}

#[tokio::test]
async fn duplicate_passthrough_upload_commit_is_idempotent() {
    let (left, right, _, right_transport) = file_system_pair(false).await;
    right_transport.duplicate_next_outbound();

    right
        .write_passthrough(TestPeer(1), "notes/today.txt", b"hello")
        .await
        .unwrap();

    assert!(entry(&left, "notes/today.txt").await.local);
    assert_eq!(left.read("notes/today.txt").await.unwrap(), b"hello");
    assert_eq!(
        left.entry("notes/today.txt")
            .await
            .unwrap()
            .providers
            .into_iter()
            .collect::<Vec<_>>(),
        [TestPeer(1)]
    );
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

    right_transport.set_online(false).await;
    right.write("notes/right.txt", b"right").await.unwrap();
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    assert!(left.entry("notes/right.txt").await.is_err());

    right_transport.set_online(true).await;
    assert!(right_transport.is_online());
    entry(&left, "notes/right.txt").await;

    left_transport.set_online(false).await;
    left.write("notes/left.txt", b"left").await.unwrap();
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    assert!(right.entry("notes/left.txt").await.is_err());

    left_transport.set_online(true).await;
    assert!(left_transport.is_online());
    entry(&right, "notes/left.txt").await;
}

#[tokio::test]
async fn syncs_tombstones_after_reconnect() {
    let (left, right, left_transport, _) = file_system_pair(false).await;

    left.write("notes/today.txt", b"hello").await.unwrap();
    entry(&right, "notes/today.txt").await;
    left_transport.set_online(false).await;
    left.delete("notes/today.txt").await.unwrap();

    left_transport.set_online(true).await;
    for _ in 0..100 {
        if right.entry("notes/today.txt").await.is_err() {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(right.entry("notes/today.txt").await.is_err());
    assert!(right.list("notes").await.unwrap().is_empty());
}

#[cfg(feature = "native")]
#[tokio::test]
async fn deleting_removes_unreferenced_blobs_and_preserves_shared_blobs() {
    let root = temp_dir("delete-gc");
    let _ = fs::remove_dir_all(&root);
    let (transport, _remote) = MemoryTransport::pair(TestPeer(1), TestPeer(2));
    let file_system: NativeTestFileSystem =
        FileSystem::new(NativeStorage::new(&root).unwrap(), TestPeer(1), transport)
            .await
            .unwrap();
    let unique = ContentHash::of(b"unique");
    let shared = ContentHash::of(b"shared");

    file_system.write("unique.txt", b"unique").await.unwrap();
    file_system.write("first.txt", b"shared").await.unwrap();
    file_system.write("second.txt", b"shared").await.unwrap();
    file_system.delete("unique.txt").await.unwrap();
    file_system.delete("first.txt").await.unwrap();

    assert!(!root.join(format!(".blobs/{unique}")).exists());
    assert!(root.join(format!(".blobs/{shared}")).exists());
    assert_eq!(file_system.read("second.txt").await.unwrap(), b"shared");
    drop(file_system);
    let _ = fs::remove_dir_all(root);
}

#[cfg(feature = "native")]
#[tokio::test]
async fn overwriting_removes_unreferenced_blobs_and_preserves_shared_blobs() {
    let root = temp_dir("overwrite-gc");
    let _ = fs::remove_dir_all(&root);
    let (transport, _remote) = MemoryTransport::pair(TestPeer(1), TestPeer(2));
    let file_system: NativeTestFileSystem =
        FileSystem::new(NativeStorage::new(&root).unwrap(), TestPeer(1), transport)
            .await
            .unwrap();
    let old = ContentHash::of(b"old");
    let shared = ContentHash::of(b"shared");

    file_system.write("replaced.txt", b"old").await.unwrap();
    file_system.write("first.txt", b"shared").await.unwrap();
    file_system.write("second.txt", b"shared").await.unwrap();
    file_system.write("replaced.txt", b"new").await.unwrap();
    file_system.write("first.txt", b"newer").await.unwrap();

    assert!(!root.join(format!(".blobs/{old}")).exists());
    assert!(root.join(format!(".blobs/{shared}")).exists());
    assert_eq!(file_system.read("second.txt").await.unwrap(), b"shared");
    drop(file_system);
    let _ = fs::remove_dir_all(root);
}

#[cfg(feature = "native")]
#[tokio::test]
async fn persists_offline_metadata_across_a_native_restart() {
    let root = env::temp_dir().join(format!("file-system-restart-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (transport, _remote) = MemoryTransport::pair(TestPeer(1), TestPeer(2));
    transport.set_online(false).await;
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

#[cfg(feature = "native")]
#[tokio::test]
async fn persists_root_and_nested_folder_metadata_across_a_native_restart() {
    let root = env::temp_dir().join(format!("file-system-folders-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let (transport, _remote) = MemoryTransport::pair(TestPeer(1), TestPeer(2));
    transport.set_online(false).await;
    let file_system: NativeTestFileSystem =
        FileSystem::new(NativeStorage::new(&root).unwrap(), TestPeer(1), transport)
            .await
            .unwrap();

    file_system.write("root.txt", b"root").await.unwrap();
    file_system
        .write("notes/projects/lidp.txt", b"nested")
        .await
        .unwrap();
    drop(file_system);

    let (left_transport, right_transport) = MemoryTransport::pair(TestPeer(1), TestPeer(2));
    let file_system: NativeTestFileSystem = FileSystem::new(
        NativeStorage::new(&root).unwrap(),
        TestPeer(1),
        left_transport,
    )
    .await
    .unwrap();
    let remote: TestFileSystem =
        FileSystem::new(InMemoryStorage::new(), TestPeer(2), right_transport)
            .await
            .unwrap();

    assert_eq!(file_system.read("root.txt").await.unwrap(), b"root");
    assert_eq!(
        file_system.read("notes/projects/lidp.txt").await.unwrap(),
        b"nested"
    );
    assert_eq!(file_system.list("").await.unwrap().len(), 1);
    assert_eq!(file_system.list("notes/projects").await.unwrap().len(), 1);

    for _ in 0..100 {
        if remote.entry("root.txt").await.is_ok()
            && remote.entry("notes/projects/lidp.txt").await.is_ok()
        {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert_eq!(
        remote.start_read("root.txt").await.unwrap().await.unwrap(),
        b"root"
    );
    assert_eq!(
        remote
            .start_read("notes/projects/lidp.txt")
            .await
            .unwrap()
            .await
            .unwrap(),
        b"nested"
    );

    drop(remote);
    drop(file_system);
    let _ = fs::remove_dir_all(root);
}

async fn next(
    stream: &mut file_system::ReadStream<file_system::Error>,
) -> Option<Result<Vec<u8>, file_system::ReadError<file_system::Error>>> {
    core::future::poll_fn(|context| Pin::new(&mut *stream).poll_next(context)).await
}
