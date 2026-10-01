use std::{io, sync::Arc, time::Duration};

use db::NativeEngine;
use iroh::{
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use management_service::DeviceRepo;
use ofdb::{IrohTransport, SessionConfig, SyncRole};

use super::BootstrapRegistry;

pub const BOOTSTRAP_ALPN: &[u8] = b"idp-bootstrap/1";
const MAX_GRANT_BYTES: usize = 256;

#[derive(Clone)]
pub struct BootstrapProtocolHandler {
    engine: Arc<NativeEngine>,
    grants: Arc<BootstrapRegistry>,
    devices: Arc<crate::router::NativeDeviceRepo>,
}

impl BootstrapProtocolHandler {
    pub fn new(
        engine: Arc<NativeEngine>,
        grants: Arc<BootstrapRegistry>,
        devices: Arc<crate::router::NativeDeviceRepo>,
    ) -> Self {
        Self {
            engine,
            grants,
            devices,
        }
    }

    async fn handle(&self, connection: Connection) -> io::Result<()> {
        let remote_id = connection.remote_id();
        let (mut send, mut recv) = connection.accept_bi().await.map_err(io::Error::other)?;
        let mut length = [0; 2];
        recv.read_exact(&mut length)
            .await
            .map_err(io::Error::other)?;
        let length = u16::from_be_bytes(length) as usize;
        if length == 0 || length > MAX_GRANT_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid bootstrap grant length",
            ));
        }
        let mut grant_id = vec![0; length];
        recv.read_exact(&mut grant_id)
            .await
            .map_err(io::Error::other)?;
        let grant_id = std::str::from_utf8(&grant_id)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid bootstrap grant"))?;
        if !self.grants.reserve(grant_id, remote_id) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "bootstrap grant denied",
            ));
        }
        let approval = async {
            let grant = self.grants.get(grant_id).ok_or_else(|| {
                io::Error::new(io::ErrorKind::PermissionDenied, "bootstrap grant expired")
            })?;
            let approved = self
                .devices
                .approve(grant.device_id, &grant.enrollment_code_hash)
                .await
                .map_err(io::Error::other)?;
            if approved.is_none()
                && !self
                    .devices
                    .find_approved_by_public_key(&remote_id.to_string())
                    .await
                    .map_err(io::Error::other)?
                    .is_some_and(|device| device.id == grant.device_id)
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "registered device approval failed",
                ));
            }
            Ok(())
        }
        .await;
        if let Err(error) = approval {
            self.grants.finish(grant_id, remote_id, false);
            return Err(error);
        }
        if let Err(error) = send.write_all(b"OK").await {
            self.grants.finish(grant_id, remote_id, false);
            return Err(io::Error::other(error));
        }
        let mut transport = IrohTransport::new(send, recv);
        let result = ofdb::synchronize(
            &self.engine,
            &mut transport,
            &SessionConfig::default(),
            SyncRole::Responder,
        )
        .await
        .map(|_| ())
        .map_err(io::Error::other);
        self.grants.finish(grant_id, remote_id, result.is_ok());
        if result.is_ok() {
            let _ = tokio::time::timeout(Duration::from_secs(10), connection.closed()).await;
        }
        result
    }
}

impl ProtocolHandler for BootstrapProtocolHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        if let Err(error) = self.handle(connection).await {
            log::debug!("bootstrap sync failed: {error:?}");
        }
        Ok(())
    }
}

impl core::fmt::Debug for BootstrapProtocolHandler {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("BootstrapProtocolHandler")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc, time::Duration};

    use db::open_native_engine;
    use iroh::{
        Endpoint, address_lookup::MemoryLookup, endpoint::Connection, endpoint::presets,
        protocol::AcceptError, protocol::ProtocolHandler,
    };
    use iroh_chain::{EndpointIdStore, Server};
    use management_service::{DeviceRepo, replica::DbDeviceRepo};
    use ofdb::{IrohTransport, SessionConfig, SyncRole};

    use super::{BOOTSTRAP_ALPN, BootstrapProtocolHandler};
    use crate::bootstrap::BootstrapRegistry;

    #[derive(Debug)]
    struct UnusedProtocol;

    impl ProtocolHandler for UnusedProtocol {
        async fn accept(&self, _connection: Connection) -> Result<(), AcceptError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn bootstrap_checks_peer_and_token_and_retries_interrupted_sync() {
        tokio::time::timeout(Duration::from_secs(60), async {
            let root = std::env::temp_dir().join(format!(
                "bootstrap-protocol-{}-{}",
                std::process::id(),
                idp_model::model::Id::now_v7()
            ));
            fs::create_dir_all(&root).expect("create test root");
            let source =
                Arc::new(open_native_engine(root.join("source.redb")).expect("open source"));
            let destination = Arc::new(
                open_native_engine(root.join("destination.redb")).expect("open destination"),
            );
            idp_model::replica::up(&source)
                .await
                .expect("initialize source control-plane schema");

            let lookup = MemoryLookup::new();
            let client = Endpoint::builder(presets::Minimal)
                .address_lookup(lookup.clone())
                .bind()
                .await
                .expect("bind joining endpoint");
            let wrong_peer = Endpoint::builder(presets::Minimal)
                .address_lookup(lookup.clone())
                .bind()
                .await
                .expect("bind wrong endpoint");

            let server_endpoint = Endpoint::builder(presets::Minimal)
                .address_lookup(lookup.clone())
                .bind()
                .await
                .expect("bind server endpoint");
            lookup.add_endpoint_info(client.addr());
            lookup.add_endpoint_info(wrong_peer.addr());
            lookup.add_endpoint_info(server_endpoint.addr());

            let devices = Arc::new(DbDeviceRepo::new(Arc::clone(&source)));
            devices
                .create(
                    "owner".to_owned(),
                    "control-plane device".to_owned(),
                    server_endpoint.id().to_string(),
                    serde_json::to_string(&server_endpoint.addr())
                        .expect("serialize control-plane endpoint"),
                    Vec::new(),
                    0,
                )
                .await
                .expect("register initial approved device");
            let enrollment_hash = vec![1, 2, 3];
            let expiry = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock is after Unix epoch")
                .as_secs() as i64
                + 900;
            let device = devices
                .create(
                    "owner".to_owned(),
                    "joining device".to_owned(),
                    client.id().to_string(),
                    serde_json::to_string(&client.addr()).expect("serialize joining endpoint"),
                    enrollment_hash.clone(),
                    expiry,
                )
                .await
                .expect("register pending device");
            assert_eq!(device.state, idp_model::contract::DeviceState::Pending);
            let registry = Arc::new(BootstrapRegistry::default());
            let grant = "test-grant".to_owned();
            registry.register(grant.clone(), client.id(), device.id, enrollment_hash);

            let allowed = EndpointIdStore::new();
            allowed.add(client.id());
            let server = Server::new(server_endpoint, allowed);
            let handler = BootstrapProtocolHandler::new(
                Arc::clone(&source),
                Arc::clone(&registry),
                Arc::clone(&devices),
            );
            let router = server.router_with_protocol(UnusedProtocol, BOOTSTRAP_ALPN, handler);

            async fn send_grant(
                endpoint: &Endpoint,
                server_addr: iroh::EndpointAddr,
                grant: &str,
            ) -> (
                iroh::endpoint::Connection,
                iroh::endpoint::SendStream,
                iroh::endpoint::RecvStream,
            ) {
                let connection = endpoint
                    .connect(server_addr, BOOTSTRAP_ALPN)
                    .await
                    .expect("connect bootstrap peer");
                let (mut send, recv) = connection.open_bi().await.expect("open bootstrap stream");
                send.write_all(&(grant.len() as u16).to_be_bytes())
                    .await
                    .expect("write grant length");
                send.write_all(grant.as_bytes()).await.expect("write grant");
                (connection, send, recv)
            }

            let server_addr = server.endpoint().addr();
            let (_connection, _send, mut recv) =
                send_grant(&wrong_peer, server_addr.clone(), &grant).await;
            assert!(recv.read_to_end(16).await.is_err());
            let (_connection, _send, mut recv) =
                send_grant(&client, server_addr.clone(), "wrong-token").await;
            assert!(recv.read_to_end(16).await.is_err());

            let mut response = [0; 2];
            let (_connection, mut send, mut recv) =
                send_grant(&client, server_addr.clone(), &grant).await;
            recv.read_exact(&mut response)
                .await
                .expect("read grant acknowledgement");
            assert_eq!(&response, b"OK");
            send.write_all(&[0, 0])
                .await
                .expect("write interrupted frame");
            send.finish().expect("finish interrupted stream");
            tokio::time::sleep(Duration::from_millis(100)).await;
            drop(destination);
            let destination = Arc::new(
                open_native_engine(root.join("destination.redb"))
                    .expect("reopen destination after interrupted bootstrap"),
            );

            let (connection, send, mut recv) = send_grant(&client, server_addr, &grant).await;
            recv.read_exact(&mut response)
                .await
                .expect("read retry acknowledgement");
            assert_eq!(&response, b"OK");
            let mut transport = IrohTransport::new(send, recv);
            ofdb::synchronize(
                &destination,
                &mut transport,
                &SessionConfig::default(),
                SyncRole::Initiator,
            )
            .await
            .expect("synchronize control-plane engine");
            drop(connection);
            let synchronized_devices = DbDeviceRepo::new(Arc::clone(&destination))
                .list()
                .await
                .expect("read synchronized devices");
            assert!(synchronized_devices.iter().any(|device| {
                device.public_key == client.id().to_string()
                    && device.state == idp_model::contract::DeviceState::Approved
            }));
            assert!(!registry.reserve(&grant, client.id()));
            server.close().await;
            drop(router);
            let _ = fs::remove_dir_all(root);
        })
        .await
        .expect("bootstrap test timed out");
    }
}
