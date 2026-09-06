use std::io::Error;

use iroh::{Endpoint, RelayMode, endpoint::presets};
use iroh_chain::{IRON_CHAIN_V1_ALPN, InMemoryEndpointIdStore, Server, ServerEvent};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    spawn,
};

const HELLO_WORLD_LINE: &str = "Hello, world!\n";

#[tokio::test]
#[test_log::test]
async fn smoke_test() -> Result<(), Error> {
    let endpoint_a = Endpoint::builder(presets::N0)
        .alpns(vec![IRON_CHAIN_V1_ALPN.to_vec()])
        .relay_mode(RelayMode::Disabled)
        .bind()
        .await
        .map_err(Error::other)?;
    log::info!("Endpoint A: {:?}", endpoint_a.id());

    let endpoint_b = Endpoint::builder(presets::N0)
        .alpns(vec![IRON_CHAIN_V1_ALPN.to_vec()])
        .relay_mode(RelayMode::Disabled)
        .bind()
        .await
        .map_err(Error::other)?;
    log::info!("Endpoint B: {:?}", endpoint_b.id());

    let store = InMemoryEndpointIdStore::default();
    store.add(endpoint_a.id());
    store.add(endpoint_b.id());

    let server_a = Server::new(endpoint_a.clone(), store.clone());
    let server_b = Server::new(endpoint_b.clone(), store.clone());

    log::info!("Start listening to A and B");
    let server_a_listener = server_a.clone();
    let server_a_handle = spawn(async move {
        server_a_listener.listen().await;
    });
    log::info!("A listening");

    let server_b_listener = server_b.clone();
    let server_b_handle = spawn(async move {
        server_b_listener.listen().await;
    });
    log::info!("B listening");

    log::info!("A addr: {:?}", endpoint_a.addr());
    log::info!("B addr: {:?}", endpoint_b.addr());

    server_b.connect(endpoint_a.addr()).await?;
    log::info!("B connected to A");

    log::info!("waiting for events from A to B");
    let peer_a = match server_b
        .event_receiver()
        .await
        .recv()
        .await
        .expect("failed to receive event from B")
    {
        ServerEvent::Connected(event_b) => event_b,
        event => panic!("unexpected event from B: {:?}", event),
    };

    // MUST write to the peer to trigger the connection to be established
    {
        let mut peer_a_writer = peer_a.writer().await;
        peer_a_writer.write_all(HELLO_WORLD_LINE.as_bytes()).await?;
    }
    log::info!("B wrote to A");

    log::info!("waiting for events from B to A");
    let event_a = server_a
        .event_receiver()
        .await
        .recv()
        .await
        .expect("failed to receive event from A");
    assert!(
        matches!(event_a, ServerEvent::Connected(_)),
        "unexpected event from A: {:?}",
        event_a
    );

    log::info!("2 way connection established");
    {
        let peer_b = server_a
            .try_get(server_b.endpoint().id())
            .expect("failed to get peer A");

        let mut peer_b_message = String::new();
        let mut peer_b_reader = BufReader::new(peer_b.reader().await);
        peer_b_reader.read_line(&mut peer_b_message).await?;
        log::info!("A read from B: {}", peer_b_message);

        assert_eq!(peer_b_message, HELLO_WORLD_LINE);
    }

    server_a.close().await;
    server_b.close().await;

    server_a_handle.await?;
    server_b_handle.await?;

    Ok(())
}
