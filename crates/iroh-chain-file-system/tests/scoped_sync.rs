use std::{fmt::Display, io::Error, time::Duration};

use file_system::{FileSystem, InMemoryStorage};
use iroh::{Endpoint, RelayMode, endpoint::presets};
use iroh_chain::{InMemoryEndpointIdStore, TUNNEL_ALPN, TunnelAuthorizer, TunnelManager, VaultId};
use iroh_chain_file_system::{EndpointIdCodec, ScopedIrohTransport};
use tokio::{spawn, time::timeout};

type ScopedTransport = ScopedIrohTransport<InMemoryEndpointIdStore, TestAuthorizer>;
type TestFileSystem = FileSystem<InMemoryStorage, EndpointIdCodec, ScopedTransport>;

#[derive(Clone)]
struct TestAuthorizer;

impl TunnelAuthorizer for TestAuthorizer {
    async fn authorize(
        &self,
        _: VaultId,
        _: iroh::EndpointId,
        _: iroh::EndpointId,
        authorization: &[u8],
    ) -> bool {
        authorization == b"authorized"
    }
}

#[tokio::test]
async fn syncs_over_an_authorized_scoped_tunnel() -> Result<(), Error> {
    let endpoint_a = endpoint().await?;
    let endpoint_b = endpoint().await?;
    let allowed = InMemoryEndpointIdStore::new();
    allowed.add(endpoint_a.id());
    allowed.add(endpoint_b.id());
    let manager_a = TunnelManager::new(endpoint_a, allowed.clone(), TestAuthorizer);
    let manager_b = TunnelManager::new(endpoint_b, allowed, TestAuthorizer);
    let listener_a = spawn_listener(manager_a.clone());
    let listener_b = spawn_listener(manager_b.clone());
    let vault_id = VaultId::new([7; 32]);
    let transport_a = ScopedIrohTransport::new(manager_a.clone(), vault_id, b"authorized".to_vec());
    let transport_b = ScopedIrohTransport::new(manager_b.clone(), vault_id, b"authorized".to_vec());
    let mut peers_a = transport_a.subscribe_peers();
    let mut peers_b = transport_b.subscribe_peers();

    transport_b.connect(manager_a.endpoint().addr()).await?;
    let peer_a = timeout(Duration::from_secs(5), peers_a.recv())
        .await
        .map_err(other)?
        .map_err(other)?;
    let peer_b = timeout(Duration::from_secs(5), peers_b.recv())
        .await
        .map_err(other)?
        .map_err(other)?;
    let file_system_a: TestFileSystem = FileSystem::new(
        InMemoryStorage::new(),
        manager_a.endpoint().id(),
        transport_a,
    )
    .await
    .map_err(other)?;
    let file_system_b: TestFileSystem = FileSystem::new(
        InMemoryStorage::new(),
        manager_b.endpoint().id(),
        transport_b,
    )
    .await
    .map_err(other)?;

    file_system_a
        .write("notes/today.txt", b"hello")
        .await
        .map_err(other)?;
    file_system_a.sync_peer(peer_a).await.map_err(other)?;
    file_system_b.sync_peer(peer_b).await.map_err(other)?;

    let entry = timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(entry) = file_system_b.entry("notes/today.txt").await {
                break entry;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(other)?;
    assert_eq!(entry.size, 5);
    assert!(!entry.local);

    manager_a.close().await;
    manager_b.close().await;
    listener_a.await.map_err(other)?;
    listener_b.await.map_err(other)?;
    Ok(())
}

#[tokio::test]
async fn rejects_an_invalid_tunnel_authorization() -> Result<(), Error> {
    let endpoint_a = endpoint().await?;
    let endpoint_b = endpoint().await?;
    let allowed = InMemoryEndpointIdStore::new();
    allowed.add(endpoint_a.id());
    allowed.add(endpoint_b.id());
    let manager_a = TunnelManager::new(endpoint_a, allowed.clone(), TestAuthorizer);
    let manager_b = TunnelManager::new(endpoint_b, allowed, TestAuthorizer);
    let listener_a = spawn_listener(manager_a.clone());

    let result = manager_b
        .connect(
            VaultId::new([7; 32]),
            manager_a.endpoint().addr(),
            b"invalid",
        )
        .await;
    assert!(result.is_err());

    manager_a.close().await;
    manager_b.close().await;
    listener_a.await.map_err(other)?;
    Ok(())
}

#[tokio::test]
async fn keeps_scopes_on_separate_streams() -> Result<(), Error> {
    let endpoint_a = endpoint().await?;
    let endpoint_b = endpoint().await?;
    let allowed = InMemoryEndpointIdStore::new();
    allowed.add(endpoint_a.id());
    allowed.add(endpoint_b.id());
    let manager_a = TunnelManager::new(endpoint_a, allowed.clone(), TestAuthorizer);
    let manager_b = TunnelManager::new(endpoint_b, allowed, TestAuthorizer);
    let listener_a = spawn_listener(manager_a.clone());
    let listener_b = spawn_listener(manager_b.clone());
    let vault_a = VaultId::new([1; 32]);
    let vault_b = VaultId::new([2; 32]);
    let transport_a1 = ScopedIrohTransport::new(manager_a.clone(), vault_a, b"authorized".to_vec());
    let transport_b1 = ScopedIrohTransport::new(manager_b.clone(), vault_a, b"authorized".to_vec());
    let transport_a2 = ScopedIrohTransport::new(manager_a.clone(), vault_b, b"authorized".to_vec());
    let transport_b2 = ScopedIrohTransport::new(manager_b.clone(), vault_b, b"authorized".to_vec());
    let mut peers_a1 = transport_a1.subscribe_peers();
    let mut peers_b1 = transport_b1.subscribe_peers();
    let mut peers_a2 = transport_a2.subscribe_peers();
    let mut peers_b2 = transport_b2.subscribe_peers();

    transport_b1.connect(manager_a.endpoint().addr()).await?;
    transport_b2.connect(manager_a.endpoint().addr()).await?;
    let peer_a1 = receive_peer(&mut peers_a1).await?;
    let peer_b1 = receive_peer(&mut peers_b1).await?;
    let _ = receive_peer(&mut peers_a2).await?;
    let _ = receive_peer(&mut peers_b2).await?;
    let file_system_a1: TestFileSystem = FileSystem::new(
        InMemoryStorage::new(),
        manager_a.endpoint().id(),
        transport_a1,
    )
    .await
    .map_err(other)?;
    let file_system_b1: TestFileSystem = FileSystem::new(
        InMemoryStorage::new(),
        manager_b.endpoint().id(),
        transport_b1,
    )
    .await
    .map_err(other)?;
    let file_system_b2: TestFileSystem = FileSystem::new(
        InMemoryStorage::new(),
        manager_b.endpoint().id(),
        transport_b2,
    )
    .await
    .map_err(other)?;

    file_system_a1
        .write("notes/private.txt", b"scope a")
        .await
        .map_err(other)?;
    file_system_a1.sync_peer(peer_a1).await.map_err(other)?;
    file_system_b1.sync_peer(peer_b1).await.map_err(other)?;
    timeout(Duration::from_secs(5), async {
        loop {
            if file_system_b1.entry("notes/private.txt").await.is_ok() {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(other)?;
    assert!(file_system_b2.entry("notes/private.txt").await.is_err());

    manager_a.close().await;
    manager_b.close().await;
    listener_a.await.map_err(other)?;
    listener_b.await.map_err(other)?;
    Ok(())
}

async fn endpoint() -> Result<Endpoint, Error> {
    Endpoint::builder(presets::N0)
        .alpns(vec![TUNNEL_ALPN.to_vec()])
        .relay_mode(RelayMode::Disabled)
        .bind()
        .await
        .map_err(other)
}

fn spawn_listener(
    manager: TunnelManager<InMemoryEndpointIdStore, TestAuthorizer>,
) -> tokio::task::JoinHandle<()> {
    spawn(async move {
        manager.listen().await;
    })
}

async fn receive_peer(
    receiver: &mut tokio::sync::broadcast::Receiver<iroh::EndpointId>,
) -> Result<iroh::EndpointId, Error> {
    timeout(Duration::from_secs(5), receiver.recv())
        .await
        .map_err(other)?
        .map_err(other)
}

fn other(error: impl Display) -> Error {
    Error::other(error.to_string())
}
