use std::{fmt::Display, io::Error, time::Duration};

use file_system::{FileSystem, InMemoryStorage};
use iroh::{Endpoint, RelayMode, endpoint::presets};
use iroh_chain::{IRON_CHAIN_V1_ALPN, InMemoryEndpointIdStore, Server};
use iroh_chain_file_system::{EndpointIdCodec, IrohTransport};
use tokio::{spawn, time::timeout};

type TestFileSystem =
    FileSystem<InMemoryStorage, EndpointIdCodec, IrohTransport<InMemoryEndpointIdStore>>;

#[tokio::test]
async fn syncs_files_after_an_iroh_peer_connects() -> Result<(), Error> {
    let endpoint_a = endpoint().await?;
    let endpoint_b = endpoint().await?;
    let allowed = InMemoryEndpointIdStore::new();
    allowed.add(endpoint_a.id());
    allowed.add(endpoint_b.id());
    let server_a = Server::new(endpoint_a.clone(), allowed.clone());
    let server_b = Server::new(endpoint_b.clone(), allowed);
    let listener_a = spawn_listener(server_a.clone());
    let listener_b = spawn_listener(server_b.clone());
    let transport_a = IrohTransport::new(server_a.clone());
    let transport_b = IrohTransport::new(server_b.clone());
    let mut peers_a = transport_a.subscribe_peers();
    let mut peers_b = transport_b.subscribe_peers();
    let file_system_a: TestFileSystem =
        FileSystem::new(InMemoryStorage::new(), endpoint_a.id(), transport_a)
            .await
            .map_err(other)?;
    let file_system_b: TestFileSystem =
        FileSystem::new(InMemoryStorage::new(), endpoint_b.id(), transport_b)
            .await
            .map_err(other)?;

    file_system_a
        .write("notes/today.txt", b"hello")
        .await
        .map_err(other)?;
    server_b.connect(endpoint_a.addr()).await?;

    let peer_a = timeout(Duration::from_secs(5), peers_a.recv())
        .await
        .map_err(other)?
        .map_err(other)?;
    let peer_b = timeout(Duration::from_secs(5), peers_b.recv())
        .await
        .map_err(other)?
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
    assert_eq!(
        file_system_b
            .start_read("notes/today.txt")
            .await
            .map_err(other)?
            .await
            .map_err(other)?,
        b"hello"
    );

    server_b.connect(endpoint_a.addr()).await?;
    let peer_a = timeout(Duration::from_secs(5), peers_a.recv())
        .await
        .map_err(other)?
        .map_err(other)?;
    let peer_b = timeout(Duration::from_secs(5), peers_b.recv())
        .await
        .map_err(other)?
        .map_err(other)?;
    file_system_a.sync_peer(peer_a).await.map_err(other)?;
    file_system_b.sync_peer(peer_b).await.map_err(other)?;
    file_system_a
        .write("notes/reconnected.txt", b"still connected")
        .await
        .map_err(other)?;

    let entry = timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(entry) = file_system_b.entry("notes/reconnected.txt").await {
                break entry;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(other)?;
    assert_eq!(entry.size, 15);
    assert_eq!(
        file_system_b
            .start_read("notes/reconnected.txt")
            .await
            .map_err(other)?
            .await
            .map_err(other)?,
        b"still connected"
    );

    server_a.close().await;
    server_b.close().await;
    listener_a.await.map_err(other)?;
    listener_b.await.map_err(other)?;
    Ok(())
}

async fn endpoint() -> Result<Endpoint, Error> {
    Endpoint::builder(presets::N0)
        .alpns(vec![IRON_CHAIN_V1_ALPN.to_vec()])
        .relay_mode(RelayMode::Disabled)
        .bind()
        .await
        .map_err(other)
}

fn other(error: impl Display) -> Error {
    Error::other(error.to_string())
}

fn spawn_listener(server: Server<InMemoryEndpointIdStore>) -> tokio::task::JoinHandle<()> {
    spawn(async move {
        server.listen().await;
    })
}
