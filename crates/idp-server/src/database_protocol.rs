use std::{future::Future, io, pin::Pin, sync::Arc};

use iroh::{
    EndpointId,
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use iroh_chain::{DATA_ALPN, Server};
use management_service::{
    DeviceRepo,
    replica::{DbDeviceRepo, DbSelectionPolicyRepo, SelectedResource},
};
use ofdb::{AutomergeRowCodec, IrohTransport, RedbKernel, SessionConfig, SyncRole, SyncTransport};
use serde::{Deserialize, Serialize};
use storage_model::StorageNamespace;
use storage_service::{DatabaseId, DatabaseRuntime};

use crate::{DeviceIdentity, data_protocol::DATABASE_STREAM_KIND};

type Devices = DbDeviceRepo<RedbKernel, AutomergeRowCodec>;
type SelectionPolicies = DbSelectionPolicyRepo<RedbKernel, AutomergeRowCodec>;
type FrameAuthorizer = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = bool> + Send>> + Send + Sync>;

const MAX_HANDSHAKE_BYTES: usize = 4096;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct DatabaseResourceDescriptor {
    pub(crate) owner_subject: String,
    pub(crate) application_id: String,
    pub(crate) database_id: String,
}

#[derive(Deserialize, Serialize)]
struct DatabaseSyncHandshake {
    resource: DatabaseResourceDescriptor,
    deleted: bool,
}

#[derive(Clone)]
pub(crate) struct DatabaseProtocolHandler {
    identity: Arc<DeviceIdentity>,
    devices: Arc<Devices>,
    policies: Arc<SelectionPolicies>,
    databases: Arc<DatabaseRuntime>,
}

impl DatabaseProtocolHandler {
    pub(crate) fn new(
        identity: Arc<DeviceIdentity>,
        devices: Arc<Devices>,
        policies: Arc<SelectionPolicies>,
        databases: Arc<DatabaseRuntime>,
    ) -> Self {
        Self {
            identity,
            devices,
            policies,
            databases,
        }
    }

    pub(crate) async fn synchronize_selected_peers(&self, manager: &Server) {
        let local_public_key = self.identity.endpoint_id().to_string();
        let devices = match self.devices.list().await {
            Ok(devices) => devices,
            Err(error) => {
                log::warn!("failed to list devices for database sync: {error}");
                return;
            }
        };
        let Some(local_device) = devices
            .iter()
            .find(|device| device.public_key == local_public_key)
        else {
            return;
        };
        let selected = match self
            .policies
            .selected_resources_for_device(local_device.id)
            .await
        {
            Ok(selected) => selected,
            Err(error) => {
                log::warn!("failed to list selected databases: {error}");
                return;
            }
        };
        for peer in manager.peers().ids() {
            let remote_public_key = peer.to_string();
            if local_public_key >= remote_public_key {
                continue;
            }
            for resource in selected
                .iter()
                .filter(|resource| resource.kind == "database")
            {
                let descriptor = descriptor(resource);
                if let Err(error) = self.synchronize_peer(manager, peer, descriptor).await {
                    log::debug!("database sync with {remote_public_key} failed: {error}");
                }
            }
        }
    }

    pub(crate) async fn synchronize_peer(
        &self,
        manager: &Server,
        peer: EndpointId,
        resource: DatabaseResourceDescriptor,
    ) -> io::Result<()> {
        let local_public_key = self.identity.endpoint_id().to_string();
        let remote_public_key = peer.to_string();
        if !self
            .authorize(&resource, &local_public_key, &remote_public_key)
            .await
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "database resource is not authorized for sync",
            ));
        }
        let connection = manager
            .connect_direct_with_alpn(peer, DATA_ALPN)
            .await
            .map_err(io::Error::other)?;
        let database_id = resource
            .database_id
            .parse::<DatabaseId>()
            .map_err(io::Error::other)?;
        let application_id = resource.application_id.parse().map_err(io::Error::other)?;
        let namespace = Namespace {
            owner_subject: resource.owner_subject.clone(),
            application_id,
        };
        let deleted = self.databases.is_tombstoned(&namespace, database_id)?;
        let (mut send, mut recv) = connection.open_bi().await.map_err(io::Error::other)?;
        send.write_all(&[DATABASE_STREAM_KIND])
            .await
            .map_err(io::Error::other)?;
        write_frame(
            &mut send,
            &serde_json::to_vec(&DatabaseSyncHandshake {
                resource: resource.clone(),
                deleted,
            })
            .map_err(io::Error::other)?,
        )
        .await?;
        if read_frame(&mut recv, MAX_HANDSHAKE_BYTES).await? != b"OK" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid database sync acknowledgement",
            ));
        }
        if deleted {
            return Ok(());
        }
        let database = self
            .databases
            .open(&namespace, database_id)
            .map_err(io::Error::other)?
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "database resource not found")
            })?;
        let kv_store = self
            .databases
            .open_kv_selected(&namespace, database_id)
            .map_err(io::Error::other)?
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "database KV store not found")
            })?;
        let handler = self.clone();
        let resource_for_guard = resource.clone();
        let authorize: FrameAuthorizer = Arc::new(move || {
            let handler = handler.clone();
            let resource = resource_for_guard.clone();
            let local_public_key = local_public_key.clone();
            let remote_public_key = remote_public_key.clone();
            Box::pin(async move {
                handler
                    .authorize(&resource, &local_public_key, &remote_public_key)
                    .await
            })
        });
        let mut transport = AuthorizedTransport {
            transport: IrohTransport::new(send, recv),
            authorize,
        };
        database
            .synchronize(
                &mut transport,
                &SessionConfig::default(),
                SyncRole::Initiator,
            )
            .await
            .map_err(io::Error::other)?;
        kv_sync::synchronize(
            &kv_store,
            &mut KvAuthorizedTransport {
                transport: &mut transport,
            },
            kv_sync::SyncRole::Initiator,
            kv_sync::Config::default(),
        )
        .await
        .map_err(|error| io::Error::other(format!("database KV sync failed: {error:?}")))?;
        Ok(())
    }

    async fn authorize(
        &self,
        resource: &DatabaseResourceDescriptor,
        local_public_key: &str,
        remote_public_key: &str,
    ) -> bool {
        let (Ok(application_id), Ok(database_id)) = (
            resource.application_id.parse(),
            resource.database_id.parse::<DatabaseId>(),
        ) else {
            return false;
        };
        if resource.owner_subject.trim().is_empty() {
            return false;
        }
        let namespace = Namespace {
            owner_subject: resource.owner_subject.clone(),
            application_id,
        };
        self.policies
            .peers_selected_for_sync(
                &self.devices,
                local_public_key,
                remote_public_key,
                &namespace.owner_subject,
                namespace.application_id,
                "database",
                database_id,
            )
            .await
            .unwrap_or(false)
    }
}

impl core::fmt::Debug for DatabaseProtocolHandler {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("DatabaseProtocolHandler")
            .finish_non_exhaustive()
    }
}

impl DatabaseProtocolHandler {
    pub(crate) async fn accept_stream(
        &self,
        connection: Connection,
        mut send: iroh::endpoint::SendStream,
        mut recv: iroh::endpoint::RecvStream,
    ) {
        let remote_public_key = connection.remote_id().to_string();
        let local_public_key = self.identity.endpoint_id().to_string();
        let handshake = match read_handshake(&mut recv).await {
            Ok(handshake) => handshake,
            Err(error) => {
                log::warn!("rejected database sync handshake: {error}");
                return;
            }
        };
        if !self
            .authorize(&handshake.resource, &local_public_key, &remote_public_key)
            .await
        {
            log::warn!("rejected unauthorized database sync stream");
            return;
        }
        let (Ok(application_id), Ok(database_id)) = (
            handshake.resource.application_id.parse(),
            handshake.resource.database_id.parse::<DatabaseId>(),
        ) else {
            return;
        };
        let namespace = Namespace {
            owner_subject: handshake.resource.owner_subject.clone(),
            application_id,
        };
        if handshake.deleted {
            if let Err(error) = self.databases.apply_tombstone(&namespace, database_id) {
                log::warn!("failed to apply database tombstone: {error}");
                return;
            }
            if let Err(error) = write_frame(&mut send, b"OK").await {
                log::warn!("failed to acknowledge database tombstone: {error}");
            }
            return;
        }
        let Some(database) = self
            .databases
            .open_selected(&namespace, database_id)
            .ok()
            .flatten()
        else {
            return;
        };
        let Some(kv_store) = self
            .databases
            .open_kv_selected(&namespace, database_id)
            .ok()
            .flatten()
        else {
            return;
        };
        if let Err(error) = write_frame(&mut send, b"OK").await {
            log::warn!("failed to acknowledge database sync stream: {error}");
            return;
        }
        let handler = self.clone();
        tokio::spawn(async move {
            let resource_for_guard = handshake.resource.clone();
            let authorize: FrameAuthorizer = Arc::new(move || {
                let handler = handler.clone();
                let resource = resource_for_guard.clone();
                let local_public_key = local_public_key.clone();
                let remote_public_key = remote_public_key.clone();
                Box::pin(async move {
                    handler
                        .authorize(&resource, &local_public_key, &remote_public_key)
                        .await
                })
            });
            let mut transport = AuthorizedTransport {
                transport: IrohTransport::new(send, recv),
                authorize,
            };
            if let Err(error) = database
                .synchronize(
                    &mut transport,
                    &SessionConfig::default(),
                    SyncRole::Responder,
                )
                .await
            {
                log::warn!("database sync session ended: {error}");
                return;
            }
            if let Err(error) = kv_sync::synchronize(
                &kv_store,
                &mut KvAuthorizedTransport {
                    transport: &mut transport,
                },
                kv_sync::SyncRole::Responder,
                kv_sync::Config::default(),
            )
            .await
            {
                log::warn!("database KV sync session ended: {error:?}");
            }
        });
    }
}

impl ProtocolHandler for DatabaseProtocolHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        loop {
            let (send, mut recv) = connection.accept_bi().await?;
            let mut kind = [0; 1];
            if recv.read_exact(&mut kind).await.is_err() {
                continue;
            }
            if kind[0] != DATABASE_STREAM_KIND {
                continue;
            }
            self.accept_stream(connection.clone(), send, recv).await;
        }
    }
}

fn descriptor(resource: &SelectedResource) -> DatabaseResourceDescriptor {
    DatabaseResourceDescriptor {
        owner_subject: resource.owner_subject.clone(),
        application_id: resource.application_id.to_string(),
        database_id: resource.resource_id.to_string(),
    }
}

struct AuthorizedTransport {
    transport: IrohTransport,
    authorize: FrameAuthorizer,
}

struct KvAuthorizedTransport<'a> {
    transport: &'a mut AuthorizedTransport,
}

impl kv_sync::SyncTransport for KvAuthorizedTransport<'_> {
    type Error = io::Error;

    async fn receive(&mut self) -> Result<Vec<u8>, Self::Error> {
        self.transport.receive().await
    }

    async fn send(&mut self, frame: Vec<u8>) -> Result<(), Self::Error> {
        self.transport.send(frame).await
    }
}

impl SyncTransport for AuthorizedTransport {
    type Error = io::Error;

    async fn receive(&mut self) -> Result<Vec<u8>, Self::Error> {
        if !(self.authorize)().await {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "database sync access revoked",
            ));
        }
        let frame = self.transport.receive().await?;
        if !(self.authorize)().await {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "database sync access revoked",
            ));
        }
        Ok(frame)
    }

    async fn send(&mut self, frame: Vec<u8>) -> Result<(), Self::Error> {
        if !(self.authorize)().await {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "database sync access revoked",
            ));
        }
        self.transport.send(frame).await
    }
}

async fn read_handshake(
    recv: &mut iroh::endpoint::RecvStream,
) -> io::Result<DatabaseSyncHandshake> {
    let frame = read_frame(recv, MAX_HANDSHAKE_BYTES).await?;
    serde_json::from_slice(&frame).map_err(io::Error::other)
}

async fn read_frame(recv: &mut iroh::endpoint::RecvStream, max: usize) -> io::Result<Vec<u8>> {
    let mut length = [0; 4];
    recv.read_exact(&mut length)
        .await
        .map_err(io::Error::other)?;
    let length = u32::from_be_bytes(length) as usize;
    if length > max {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "database sync handshake too large",
        ));
    }
    let mut frame = vec![0; length];
    recv.read_exact(&mut frame)
        .await
        .map_err(io::Error::other)?;
    Ok(frame)
}

async fn write_frame(send: &mut iroh::endpoint::SendStream, frame: &[u8]) -> io::Result<()> {
    if frame.len() > MAX_HANDSHAKE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "database sync handshake too large",
        ));
    }
    send.write_all(&(frame.len() as u32).to_be_bytes())
        .await
        .map_err(io::Error::other)?;
    send.write_all(frame).await.map_err(io::Error::other)
}

struct Namespace {
    owner_subject: String,
    application_id: idp_model::model::Id,
}

impl StorageNamespace for Namespace {
    fn user_sub(&self) -> &str {
        &self.owner_subject
    }
    fn application_id(&self) -> idp_model::model::Id {
        self.application_id
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc, time::Duration};

    use db::open_native_engine;
    use iroh::{Endpoint, address_lookup::MemoryLookup, endpoint::presets};
    use iroh_chain::{DATA_ALPN, EndpointIdStore, Server};
    use management_service::{
        DeviceRepo,
        replica::{DbDeviceRepo, DbSelectionPolicyRepo, SelectionPolicy},
    };
    use ofdb::SqlTranslator;
    use ofdb_sync::{apply_sync_state_batch_for, export_sync_state_for};
    use storage_service::DatabaseRuntime;

    use super::{DatabaseProtocolHandler, DatabaseResourceDescriptor, DeviceIdentity};

    async fn replicate_management_state(source: &db::NativeEngine, destination: &db::NativeEngine) {
        let state = export_sync_state_for(source)
            .await
            .expect("export management replica state");
        apply_sync_state_batch_for(destination, state)
            .await
            .expect("apply management replica state");
    }

    async fn send_raw_descriptor(
        server: &Server,
        peer: iroh::EndpointId,
        length: u32,
        body: &[u8],
    ) {
        let connection = server
            .connect_direct_with_alpn(peer, DATA_ALPN)
            .await
            .expect("connect for raw handshake test");
        let (mut send, _receive) = connection.open_bi().await.expect("open raw test stream");
        send.write_all(&[super::DATABASE_STREAM_KIND])
            .await
            .expect("write database stream kind");
        send.write_all(&length.to_be_bytes())
            .await
            .expect("write raw test length");
        send.write_all(body).await.expect("write raw test body");
        send.finish().expect("finish raw test stream");
        drop(connection);
    }

    async fn interrupt_sync_after_ack(
        server: &Server,
        peer: iroh::EndpointId,
        descriptor: &DatabaseResourceDescriptor,
    ) {
        let connection = server
            .connect_direct_with_alpn(peer, DATA_ALPN)
            .await
            .expect("connect for interrupted sync test");
        let (mut send, mut receive) = connection
            .open_bi()
            .await
            .expect("open interrupted sync stream");
        send.write_all(&[super::DATABASE_STREAM_KIND])
            .await
            .expect("write database stream kind");
        let handshake = serde_json::to_vec(&super::DatabaseSyncHandshake {
            resource: descriptor.clone(),
            deleted: false,
        })
        .expect("serialize resource descriptor");
        super::write_frame(&mut send, &handshake)
            .await
            .expect("send authorized resource descriptor");
        assert_eq!(
            super::read_frame(&mut receive, super::MAX_HANDSHAKE_BYTES)
                .await
                .expect("read handshake acknowledgement"),
            b"OK"
        );
        send.write_all(&[0, 0])
            .await
            .expect("send partial sync frame length");
        send.finish().expect("interrupt sync stream");
        drop(connection);
    }

    #[tokio::test]
    async fn selected_database_sync_provisions_receiver_and_rejects_wrong_namespace() {
        tokio::time::timeout(Duration::from_secs(240), async {
            let root = std::env::temp_dir().join(format!(
                "database-protocol-{}-{}",
                std::process::id(),
                idp_model::model::Id::now_v7()
            ));
            fs::create_dir_all(&root).expect("create test root");
            let engine = Arc::new(
                open_native_engine(root.join("management.redb")).expect("open management database"),
            );
            idp_model::replica::up(&engine)
                .await
                .expect("initialize management schema");
            let devices = Arc::new(DbDeviceRepo::new(Arc::clone(&engine)));
            let policies = Arc::new(DbSelectionPolicyRepo::new(Arc::clone(&engine)));

            let lookup = MemoryLookup::new();
            let secret_key_a = iroh::SecretKey::generate();
            let secret_key_b = iroh::SecretKey::generate();
            let endpoint_a = Endpoint::builder(presets::Minimal)
                .secret_key(secret_key_a.clone())
                .address_lookup(lookup.clone())
                .bind()
                .await
                .expect("bind first endpoint");
            let endpoint_b = Endpoint::builder(presets::Minimal)
                .secret_key(secret_key_b.clone())
                .address_lookup(lookup.clone())
                .bind()
                .await
                .expect("bind second endpoint");
            lookup.add_endpoint_info(endpoint_a.addr());
            lookup.add_endpoint_info(endpoint_b.addr());
            let id_a = endpoint_a.id();
            let id_b = endpoint_b.id();
            let device_a = devices
                .create(
                    "owner".into(),
                    "first".into(),
                    id_a.to_string(),
                    "first-address".into(),
                    vec![],
                    0,
                )
                .await
                .expect("create first approved device");
            let pending_b = devices
                .create_pairing(
                    "second".into(),
                    id_b.to_string(),
                    "second-address".into(),
                    id_a.to_string(),
                )
                .await
                .expect("create paired device");
            devices
                .approve_pairing(pending_b.id)
                .await
                .expect("approve paired device")
                .expect("paired device exists");
            let application_id = idp_model::model::Id::now_v7();
            let database_root = root.join("databases");
            let source_runtime = Arc::new(
                DatabaseRuntime::new(database_root.join("source")).expect("create source runtime"),
            );
            let receiver_runtime = Arc::new(
                DatabaseRuntime::new(database_root.join("receiver"))
                    .expect("create receiver runtime"),
            );
            let scope = super::Namespace {
                owner_subject: "owner".into(),
                application_id,
            };
            let (resource, source_db) = source_runtime
                .create(&scope, Some("replicated".into()))
                .expect("create source database");
            let (second_resource, second_source_db) = source_runtime
                .create(&scope, Some("replicated".into()))
                .expect("create second source database with duplicate name");
            second_source_db
                .translate_and_execute(
                    "CREATE TABLE second_records (id UUID PRIMARY KEY)",
                    &SqlTranslator,
                )
                .await
                .expect("create second database schema");
            source_db
                .translate_and_execute("CREATE TABLE records (id UUID PRIMARY KEY)", &SqlTranslator)
                .await
                .expect("create source schema");
            source_db
                .translate_and_execute(
                    "INSERT INTO records VALUES (CAST('018f0f8e-7b6d-7c4a-8f12-123456789abc' AS UUID))",
                    &SqlTranslator,
                )
                .await
                .expect("insert source row");
            let source_kv = source_runtime
                .open_kv_selected(&scope, resource.id)
                .expect("source KV store opens")
                .expect("source KV store is available");
            let mut source_kv_transaction = source_kv
                .transaction()
                .await
                .expect("source KV transaction starts");
            source_kv_transaction
                .set("replicated-key", vec![7, 8, 9], None)
                .await
                .expect("write source KV value");
            source_kv_transaction
                .commit()
                .await
                .expect("commit source KV value");
            for device in [&device_a, &pending_b] {
                policies
                    .set_prevalidated(SelectionPolicy {
                        device_id: device.id,
                        owner_subject: "owner".into(),
                        application_id: Some(application_id),
                        selected_kind: Some("database".into()),
                        selected_id: Some(resource.id),
                        admin_allowed: true,
                    })
                    .await
                    .expect("select database on device");
            }
            let receiver_engine = Arc::new(
                open_native_engine(root.join("receiver-management.redb"))
                    .expect("open receiver management database"),
            );
            replicate_management_state(&engine, &receiver_engine).await;
            let receiver_devices = Arc::new(DbDeviceRepo::new(Arc::clone(&receiver_engine)));
            let receiver_policies = Arc::new(DbSelectionPolicyRepo::new(Arc::clone(
                &receiver_engine,
            )));

            let peers_a = EndpointIdStore::new();
            peers_a.replace([id_b]);
            let peers_b = EndpointIdStore::new();
            peers_b.replace([id_a]);
            let server_a = Server::new(endpoint_a.clone(), peers_a);
            let server_b = Server::new(endpoint_b.clone(), peers_b);
            let handler_a = DatabaseProtocolHandler::new(
                Arc::new(DeviceIdentity::new(endpoint_a, secret_key_a)),
                Arc::clone(&devices),
                Arc::clone(&policies),
                Arc::clone(&source_runtime),
            );
            let handler_b = DatabaseProtocolHandler::new(
                Arc::new(DeviceIdentity::new(endpoint_b, secret_key_b)),
                Arc::clone(&receiver_devices),
                Arc::clone(&receiver_policies),
                Arc::clone(&receiver_runtime),
            );
            let router_a = server_a.router(handler_a.clone());
            let router_b = server_b.router(handler_b.clone());
            send_raw_descriptor(
                &server_a,
                id_b,
                (super::MAX_HANDSHAKE_BYTES + 1) as u32,
                &[],
            )
            .await;
            send_raw_descriptor(&server_a, id_b, 1, b"{").await;
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert!(
                receiver_runtime
                    .get(&scope, resource.id)
                    .expect("invalid handshakes do not error catalog lookup")
                    .is_none(),
                "invalid handshakes must not provision or apply a database"
            );

            for device in [&device_a, &pending_b] {
                policies
                    .set_admin_allowed_prevalidated(device.id, "owner", false)
                    .await
                    .expect("restrict selected resource by admin policy");
            }
            replicate_management_state(&engine, &receiver_engine).await;
            let admin_restricted = serde_json::to_vec(&DatabaseResourceDescriptor {
                owner_subject: "owner".into(),
                application_id: application_id.to_string(),
                database_id: resource.id.to_string(),
            })
            .expect("serialize admin-restricted descriptor");
            send_raw_descriptor(
                &server_a,
                id_b,
                admin_restricted.len() as u32,
                &admin_restricted,
            )
            .await;
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert!(
                receiver_runtime
                    .get(&scope, resource.id)
                    .expect("admin denial does not error catalog lookup")
                    .is_none(),
                "admin-restricted resources must not be provisioned"
            );

            for device in [&device_a, &pending_b] {
                policies
                    .set_admin_allowed_prevalidated(device.id, "owner", true)
                    .await
                    .expect("restore administrator selection permission");
                policies
                    .set_prevalidated(SelectionPolicy {
                        device_id: device.id,
                        owner_subject: "owner".into(),
                        application_id: Some(application_id),
                        selected_kind: Some("filesystem".into()),
                        selected_id: Some(resource.id),
                        admin_allowed: true,
                    })
                    .await
                    .expect("select a different resource kind");
            }
            replicate_management_state(&engine, &receiver_engine).await;
            let wrong_kind = serde_json::to_vec(&DatabaseResourceDescriptor {
                owner_subject: "owner".into(),
                application_id: application_id.to_string(),
                database_id: resource.id.to_string(),
            })
            .expect("serialize wrong-kind descriptor");
            send_raw_descriptor(&server_a, id_b, wrong_kind.len() as u32, &wrong_kind).await;
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert!(
                receiver_runtime
                    .get(&scope, resource.id)
                    .expect("wrong-kind denial does not error catalog lookup")
                    .is_none(),
                "a filesystem selection must not provision a database"
            );

            for device in [&device_a, &pending_b] {
                policies
                    .set_prevalidated(SelectionPolicy {
                        device_id: device.id,
                        owner_subject: "owner".into(),
                        application_id: Some(application_id),
                        selected_kind: Some("database".into()),
                        selected_id: Some(resource.id),
                        admin_allowed: true,
                    })
                    .await
                    .expect("restore first database selection for sync tests");
                policies
                    .set_prevalidated(SelectionPolicy {
                        device_id: device.id,
                        owner_subject: "owner".into(),
                        application_id: Some(application_id),
                        selected_kind: Some("database".into()),
                        selected_id: Some(second_resource.id),
                        admin_allowed: true,
                    })
                    .await
                    .expect("select second database independently");
            }
            replicate_management_state(&engine, &receiver_engine).await;

            let wrong_namespace = DatabaseResourceDescriptor {
                owner_subject: "other-owner".into(),
                application_id: application_id.to_string(),
                database_id: resource.id.to_string(),
            };
            assert_eq!(
                handler_a
                    .synchronize_peer(&server_a, id_b, wrong_namespace)
                    .await
                    .expect_err("wrong namespace is denied")
                    .kind(),
                std::io::ErrorKind::PermissionDenied
            );
            assert_eq!(
                handler_a
                    .synchronize_peer(
                        &server_a,
                        id_b,
                        DatabaseResourceDescriptor {
                            owner_subject: "owner".into(),
                            application_id: application_id.to_string(),
                            database_id: idp_model::model::Id::now_v7().to_string(),
                        },
                    )
                    .await
                    .expect_err("unselected database ID is denied")
                    .kind(),
                std::io::ErrorKind::PermissionDenied
            );
            interrupt_sync_after_ack(
                &server_a,
                id_b,
                &DatabaseResourceDescriptor {
                    owner_subject: "owner".into(),
                    application_id: application_id.to_string(),
                    database_id: resource.id.to_string(),
                },
            )
            .await;
            tokio::time::sleep(Duration::from_millis(100)).await;
            let interrupted_db = receiver_runtime
                .open(&scope, resource.id)
                .expect("database opened by authorized handshake")
                .expect("authorized resource is provisioned");
            assert!(
                interrupted_db.table_schema("records").await.is_err(),
                "an incomplete frame must not apply database state"
            );
            handler_a
                .synchronize_peer(
                    &server_a,
                    id_b,
                    DatabaseResourceDescriptor {
                        owner_subject: "owner".into(),
                        application_id: application_id.to_string(),
                        database_id: resource.id.to_string(),
                    },
                )
                .await
                .expect("selected database sync succeeds");
            handler_a
                .synchronize_peer(
                    &server_a,
                    id_b,
                    DatabaseResourceDescriptor {
                        owner_subject: "owner".into(),
                        application_id: application_id.to_string(),
                        database_id: resource.id.to_string(),
                    },
                )
                .await
                .expect("reconnect and repair are idempotent");
            let receiver_db = receiver_runtime
                .open(&scope, resource.id)
                .expect("receiver database opens")
                .expect("authorized sync provisions matching ID");
            assert_eq!(
                receiver_db
                    .table_schema("records")
                    .await
                    .expect("replicated schema exists")
                    .name,
                "records"
            );
            let rows = receiver_db
                .translate_and_execute("SELECT * FROM records", &SqlTranslator)
                .await
                .expect("replicated rows are readable");
            assert_eq!(rows[0].rows.len(), 1);
            handler_a
                .synchronize_peer(
                    &server_a,
                    id_b,
                    DatabaseResourceDescriptor {
                        owner_subject: "owner".into(),
                        application_id: application_id.to_string(),
                        database_id: second_resource.id.to_string(),
                    },
                )
                .await
                .expect("second selected database syncs independently");
            let receiver_second_db = receiver_runtime
                .open(&scope, second_resource.id)
                .expect("second receiver database opens")
                .expect("second selected database provisions its matching ID");
            assert_eq!(
                receiver_second_db
                    .table_schema("second_records")
                    .await
                    .expect("second database schema replicates")
                    .name,
                "second_records"
            );
            assert!(
                receiver_runtime
                    .get(&scope, resource.id)
                    .expect("first database remains independently addressable")
                    .is_some()
            );
            let receiver_kv = receiver_runtime
                .open_kv_selected(&scope, resource.id)
                .expect("receiver KV store opens")
                .expect("authorized sync provisions receiver KV store");
            let receiver_kv_transaction = receiver_kv
                .transaction()
                .await
                .expect("receiver KV transaction starts");
            assert_eq!(
                receiver_kv_transaction
                    .get("replicated-key", 0)
                    .await
                    .expect("read replicated KV value"),
                Some(vec![7, 8, 9])
            );
            receiver_kv_transaction
                .rollback()
                .await
                .expect("rollback receiver KV read transaction");
            assert!(
                source_runtime
                    .delete(&scope, resource.id)
                    .expect("owner tombstones source database")
            );
            handler_a
                .synchronize_peer(
                    &server_a,
                    id_b,
                    DatabaseResourceDescriptor {
                        owner_subject: "owner".into(),
                        application_id: application_id.to_string(),
                        database_id: resource.id.to_string(),
                    },
                )
                .await
                .expect("selected database tombstone replicates");
            assert!(
                receiver_runtime
                    .get(&scope, resource.id)
                    .expect("receiver tombstone lookup succeeds")
                    .is_none()
            );
            assert!(
                receiver_runtime
                    .open_selected(&scope, resource.id)
                    .expect("receiver rejects tombstoned resource")
                    .is_none()
            );
            let restarted_receiver = DatabaseRuntime::new(database_root.join("receiver"))
                .expect("reopen receiver runtime after deletion");
            assert!(
                restarted_receiver
                    .open_selected(&scope, resource.id)
                    .expect("restart preserves receiver tombstone")
                    .is_none()
            );
            assert!(
                receiver_runtime
                    .get(&scope, second_resource.id)
                    .expect("other database remains available")
                    .is_some()
            );
            for device in [&device_a, &pending_b] {
                policies
                    .set_prevalidated(SelectionPolicy {
                        device_id: device.id,
                        owner_subject: "owner".into(),
                        application_id: None,
                        selected_kind: None,
                        selected_id: None,
                        admin_allowed: true,
                    })
                    .await
                    .expect("deselect after policy convergence");
            }
            let revoked_resource = DatabaseResourceDescriptor {
                owner_subject: "owner".into(),
                application_id: application_id.to_string(),
                database_id: resource.id.to_string(),
            };
            assert!(
                handler_b
                    .authorize(&revoked_resource, &id_b.to_string(), &id_a.to_string())
                    .await,
                "receiver's stale local policy permits sync before revocation converges"
            );
            replicate_management_state(&engine, &receiver_engine).await;
            assert!(
                !handler_b
                    .authorize(&revoked_resource, &id_b.to_string(), &id_a.to_string())
                    .await,
                "receiver denies sync after revocation converges"
            );
            assert_eq!(
                handler_a
                    .synchronize_peer(&server_a, id_b, revoked_resource)
                    .await
                    .expect_err("converged revocation blocks a new sync")
                    .kind(),
                std::io::ErrorKind::PermissionDenied
            );

            server_a.close().await;
            server_b.close().await;
            drop((router_a, router_b));
            drop((
                receiver_db,
                interrupted_db,
                source_db,
                handler_a,
                receiver_runtime,
                server_a,
                server_b,
            ));
            fs::remove_dir_all(root).expect("remove test root");
        })
        .await
        .expect("Iroh database sync test completes");
    }
}
