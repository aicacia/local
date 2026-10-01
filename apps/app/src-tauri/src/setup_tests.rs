use std::{
    io,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{Json, routing::post};
use db::open_native_engine;
use idp_model::contract::{SetupBootstrapRegistration, SetupJoinRequest};
use idp_server::{AppConfig, DeviceIdentity};
use iroh::{
    Endpoint, SecretKey,
    address_lookup::MemoryLookup,
    endpoint::Connection,
    endpoint::presets,
    protocol::{AcceptError, ProtocolHandler},
};
use tokio::net::TcpListener;

use super::{SetupState, router};

const BOOTSTRAP_ALPN: &[u8] = b"idp-bootstrap/1";

#[derive(Clone, Debug)]
struct AcknowledgeAndStall {
    acknowledged: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl ProtocolHandler for AcknowledgeAndStall {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let result = async {
            let (mut send, mut recv) = connection.accept_bi().await.map_err(io::Error::other)?;
            let mut length = [0; 2];
            recv.read_exact(&mut length)
                .await
                .map_err(io::Error::other)?;
            let mut grant = vec![0; u16::from_be_bytes(length) as usize];
            recv.read_exact(&mut grant)
                .await
                .map_err(io::Error::other)?;
            send.write_all(b"OK").await?;
            if let Some(acknowledged) = self
                .acknowledged
                .lock()
                .expect("acknowledgement lock poisoned")
                .take()
            {
                let _ = acknowledged.send(());
            }
            std::future::pending::<io::Result<()>>().await
        }
        .await;
        result.map_err(AcceptError::from_err)
    }
}

#[tokio::test]
async fn join_fails_if_sync_is_interrupted_after_grant_acknowledgement() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let lookup = MemoryLookup::new();
        let client_key = SecretKey::generate();
        let client = Endpoint::builder(presets::Minimal)
            .secret_key(client_key.clone())
            .address_lookup(lookup.clone())
            .bind()
            .await
            .expect("bind joining endpoint");
        let server = Endpoint::builder(presets::Minimal)
            .address_lookup(lookup.clone())
            .bind()
            .await
            .expect("bind bootstrap peer");
        lookup.add_endpoint_info(client.addr());
        lookup.add_endpoint_info(server.addr());
        server.set_alpns(vec![BOOTSTRAP_ALPN.to_vec()]);
        let (acknowledged_tx, acknowledged_rx) = tokio::sync::oneshot::channel();
        let handler = AcknowledgeAndStall {
            acknowledged: Arc::new(Mutex::new(Some(acknowledged_tx))),
        };
        let peer_task = tokio::spawn({
            let server = server.clone();
            async move {
                while let Some(incoming) = server.accept().await {
                    let handler = handler.clone();
                    tokio::spawn(async move {
                        if let Ok(accepting) = incoming.accept()
                            && let Ok(connection) = accepting.await
                        {
                            let _ = handler.accept(connection).await;
                        }
                    });
                }
            }
        });

        let root = std::env::temp_dir().join(format!("lidp-join-test-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&root)
            .await
            .expect("create test directory");
        let database =
            Arc::new(open_native_engine(root.join("join.redb")).expect("open test database"));
        let state = SetupState {
            database,
            app_config: Arc::new(AppConfig::default()),
            device_identity: Arc::new(DeviceIdentity::new(client, client_key)),
        };
        let registration_peer = server.clone();
        let app = router(state).route(
            "/setup/bootstrap",
            post(move || async move {
                Json(SetupBootstrapRegistration {
                    grant: "local-test-grant".to_owned(),
                    endpoint_id: registration_peer.id().to_string(),
                    endpoint_addr: serde_json::to_string(&registration_peer.addr())
                        .expect("serialize peer address"),
                    expires_at: 0,
                })
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local HTTP server");
        let address = listener.local_addr().expect("read local HTTP address");
        let http_task = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve local setup routes");
        });
        let request_task = tokio::spawn(async move {
            reqwest::Client::new()
                .post(format!("http://{address}/setup/join"))
                .bearer_auth("test")
                .json(&SetupJoinRequest {
                    device_name: "joining device".to_owned(),
                    idp_url: format!("http://{address}"),
                })
                .send()
                .await
        });

        tokio::time::timeout(Duration::from_secs(5), acknowledged_rx)
            .await
            .expect("bootstrap peer receives and acknowledges its grant")
            .expect("acknowledgement is sent");
        assert!(
            !request_task.is_finished(),
            "join returned before synchronization completed"
        );
        server.close().await;
        let response = tokio::time::timeout(Duration::from_secs(5), request_task)
            .await
            .expect("join route returns after peer interruption")
            .expect("join request task completes")
            .expect("receive join response");
        assert!(
            !response.status().is_success(),
            "interrupted synchronization must fail"
        );
        http_task.abort();
        peer_task.abort();
        let _ = tokio::fs::remove_dir_all(root).await;
    })
    .await
    .expect("join synchronization test timed out");
}
