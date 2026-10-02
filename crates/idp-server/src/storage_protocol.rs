use std::sync::Arc;

use file_system::{FileSystemId, IrohFileTransport, IrohResourceDescriptor};
use iroh::{
    EndpointId,
    endpoint::{Connection, RecvStream, SendStream},
    protocol::{AcceptError, ProtocolHandler},
};
use iroh_chain::{DATA_ALPN, Server};
use management_service::{
    DeviceRepo,
    replica::{DbDeviceRepo, DbSelectionPolicyRepo, SelectedResource},
};
use ofdb::{AutomergeRowCodec, RedbKernel};
use storage_model::StorageNamespace;
use storage_service::ScopedFileSystemRuntime;

use crate::DeviceIdentity;

type Devices = DbDeviceRepo<RedbKernel, AutomergeRowCodec>;

type SelectionPolicies = DbSelectionPolicyRepo<RedbKernel, AutomergeRowCodec>;

#[derive(Clone)]
pub(crate) struct StorageProtocolHandler {
    identity: Arc<DeviceIdentity>,
    devices: Arc<Devices>,
    policies: Arc<SelectionPolicies>,
    file_systems: Arc<ScopedFileSystemRuntime<EndpointId>>,
}

impl StorageProtocolHandler {
    pub(crate) fn new(
        identity: Arc<DeviceIdentity>,
        devices: Arc<Devices>,
        policies: Arc<SelectionPolicies>,
        file_systems: Arc<ScopedFileSystemRuntime<EndpointId>>,
    ) -> Self {
        Self {
            identity,
            devices,
            policies,
            file_systems,
        }
    }

    pub(crate) async fn synchronize_selected_peers(&self, manager: &Server) {
        let local_public_key = self.identity.endpoint_id().to_string();
        let devices = match self.devices.list().await {
            Ok(devices) => devices,
            Err(error) => {
                log::warn!("failed to list devices for filesystem sync: {error}");
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
                log::warn!("failed to list selected filesystems: {error}");
                return;
            }
        };
        let selected_filesystems = selected
            .iter()
            .filter(|resource| resource.kind == "filesystem")
            .filter_map(|resource| {
                FileSystemId::parse(&resource.resource_id.to_string())
                    .ok()
                    .map(|resource_id| {
                        (
                            resource.owner_subject.clone(),
                            resource.application_id,
                            resource_id,
                        )
                    })
            })
            .collect::<Vec<_>>();
        if let Err(error) = self
            .file_systems
            .remove_unselected_projected_resources(&selected_filesystems)
            .await
        {
            log::warn!("failed to evict deselected filesystem copies: {error}");
        }
        for peer in manager.peers().ids() {
            let remote_public_key = peer.to_string();
            if local_public_key >= remote_public_key {
                continue;
            }
            for resource in selected
                .iter()
                .filter(|resource| resource.kind == "filesystem")
            {
                let descriptor = descriptor(resource);
                if !self
                    .authorize(&descriptor, &local_public_key, &remote_public_key)
                    .await
                {
                    continue;
                }
                if let Err(error) = self.synchronize_peer(manager, peer, descriptor).await {
                    log::debug!("filesystem sync with {remote_public_key} failed: {error}");
                }
            }
        }
    }

    async fn synchronize_peer(
        &self,
        manager: &Server,
        peer: EndpointId,
        resource: IrohResourceDescriptor,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let application_id = resource.application_id.parse()?;
        let filesystem_id = FileSystemId::parse(&resource.filesystem_id)?;
        let namespace = Namespace {
            owner_subject: resource.owner_subject.clone(),
            application_id,
        };
        let deleted = self
            .file_systems
            .is_tombstoned(&namespace, filesystem_id)
            .await
            .map_err(std::io::Error::other)?;
        let connection = manager.connect_direct_with_alpn(peer, DATA_ALPN).await?;
        let handler = self.clone();
        let local_public_key = self.identity.endpoint_id().to_string();
        let remote_public_key = peer.to_string();
        let transport = if deleted {
            IrohFileTransport::open_tombstone_authorized(&connection, resource, move |resource| {
                let handler = handler.clone();
                let local_public_key = local_public_key.clone();
                let remote_public_key = remote_public_key.clone();
                async move {
                    handler
                        .authorize(&resource, &local_public_key, &remote_public_key)
                        .await
                }
            })
            .await?
        } else {
            IrohFileTransport::open_authorized(&connection, resource, move |resource| {
                let handler = handler.clone();
                let local_public_key = local_public_key.clone();
                let remote_public_key = remote_public_key.clone();
                async move {
                    handler
                        .authorize(&resource, &local_public_key, &remote_public_key)
                        .await
                }
            })
            .await?
        };
        if deleted {
            transport.close();
            return Ok(());
        }
        let file_system = self
            .file_systems
            .open_resource(&namespace, filesystem_id)
            .await
            .map_err(std::io::Error::other)?;
        file_system.sync_peer(transport).await?;
        Ok(())
    }
}

impl core::fmt::Debug for StorageProtocolHandler {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("StorageProtocolHandler")
            .finish_non_exhaustive()
    }
}

impl StorageProtocolHandler {
    pub(crate) async fn accept_stream(
        &self,
        connection: Connection,
        send: SendStream,
        recv: RecvStream,
    ) {
        let remote_public_key = connection.remote_id().to_string();
        let local_public_key = self.identity.endpoint_id().to_string();
        let handler = self.clone();
        let (transport, resource, deleted) =
            match IrohFileTransport::accept_authorized_after_marker(
                &connection,
                send,
                recv,
                move |resource| {
                    let handler = handler.clone();
                    let local_public_key = local_public_key.clone();
                    let remote_public_key = remote_public_key.clone();
                    async move {
                        handler
                            .authorize(&resource, &local_public_key, &remote_public_key)
                            .await
                    }
                },
            )
            .await
            {
                Ok(accepted) => accepted,
                Err(error) => {
                    log::warn!("rejected filesystem sync stream: {error}");
                    return;
                }
            };

        let Ok(application_id) = resource.application_id.parse() else {
            return;
        };
        let Ok(filesystem_id) = FileSystemId::parse(&resource.filesystem_id) else {
            return;
        };
        let namespace = Namespace {
            owner_subject: resource.owner_subject,
            application_id,
        };
        if deleted {
            if let Err(error) = self
                .file_systems
                .apply_deletion_tombstone(&namespace, filesystem_id)
                .await
            {
                log::warn!("failed to apply filesystem deletion tombstone: {error}");
            }
            transport.close();
            return;
        }
        if let Err(error) = self
            .file_systems
            .register_selected_resource(&namespace, filesystem_id)
            .await
        {
            log::warn!("failed to register selected filesystem: {error}");
            return;
        }
        let file_system = match self
            .file_systems
            .open_resource(&namespace, filesystem_id)
            .await
        {
            Ok(file_system) => file_system,
            Err(error) => {
                log::warn!("failed to open selected filesystem: {error}");
                return;
            }
        };
        tokio::spawn(async move {
            if let Err(error) = file_system.sync_peer(transport).await {
                log::warn!("filesystem sync session ended: {error}");
            }
        });
    }
}

impl ProtocolHandler for StorageProtocolHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        loop {
            let (send, mut recv) = connection.accept_bi().await?;
            let mut kind = [0; 1];
            if recv.read_exact(&mut kind).await.is_err()
                || kind[0] != file_system::FILESYSTEM_STREAM_KIND
            {
                continue;
            }
            self.accept_stream(connection.clone(), send, recv).await;
        }
    }
}

fn descriptor(resource: &SelectedResource) -> IrohResourceDescriptor {
    IrohResourceDescriptor {
        owner_subject: resource.owner_subject.clone(),
        application_id: resource.application_id.to_string(),
        filesystem_id: resource.resource_id.to_string(),
    }
}

impl StorageProtocolHandler {
    async fn authorize(
        &self,
        resource: &IrohResourceDescriptor,
        local_public_key: &str,
        remote_public_key: &str,
    ) -> bool {
        let Ok(application_id) = resource.application_id.parse() else {
            return false;
        };
        let Ok(filesystem_id) = FileSystemId::parse(&resource.filesystem_id) else {
            return false;
        };
        if resource.owner_subject.trim().is_empty() {
            return false;
        }
        let namespace = Namespace {
            owner_subject: resource.owner_subject.clone(),
            application_id,
        };
        let authorized = self
            .policies
            .peers_selected_for_sync(
                &self.devices,
                local_public_key,
                remote_public_key,
                &namespace.owner_subject,
                namespace.application_id,
                "filesystem",
                filesystem_id.as_uuid(),
            )
            .await
            .unwrap_or(false);
        if !authorized {
            return false;
        }
        authorized
    }
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
    use std::{
        fs,
        sync::Arc,
        time::{Duration, Instant},
    };

    use db::open_native_engine;

    use iroh::{Endpoint, address_lookup::MemoryLookup, endpoint::presets};
    use iroh_chain::{DATA_ALPN, EndpointIdStore, Server};
    use management_service::{
        DeviceRepo,
        replica::{DbDeviceRepo, DbSelectionPolicyRepo, SelectionPolicy},
    };
    use ofdb::{AutomergeRowCodec, RedbKernel, SqlTranslator};
    use storage_service::{DatabaseRuntime, ScopedFileSystemRuntime};

    use super::{DeviceIdentity, Namespace, StorageProtocolHandler};
    use crate::{data_protocol::DataProtocolHandler, database_protocol::DatabaseProtocolHandler};

    async fn send_raw_filesystem_frame(
        server: &Server,
        peer: iroh::EndpointId,
        length: u32,
        body: &[u8],
    ) {
        let connection = server
            .connect_direct_with_alpn(peer, DATA_ALPN)
            .await
            .expect("connect for raw filesystem frame");
        let (mut send, _receive) = connection.open_bi().await.expect("open raw stream");
        send.write_all(&[file_system::FILESYSTEM_STREAM_KIND])
            .await
            .expect("write filesystem stream kind");
        send.write_all(&length.to_be_bytes())
            .await
            .expect("write raw frame length");
        send.write_all(body).await.expect("write raw frame body");
        send.finish().expect("finish raw stream");
        drop(connection);
    }

    #[tokio::test]
    async fn selected_filesystem_sync_provisions_only_policy_authorized_resource() {
        tokio::time::timeout(Duration::from_secs(300), async {
            let root = std::env::temp_dir().join(format!(
                "storage-protocol-{}-{}",
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
            let devices = Arc::new(DbDeviceRepo::<RedbKernel, AutomergeRowCodec>::new(
                Arc::clone(&engine),
            ));
            let policies = Arc::new(DbSelectionPolicyRepo::new(engine));
            let lookup = MemoryLookup::new();
            let secret_key_a = iroh::SecretKey::generate();
            let secret_key_b = iroh::SecretKey::generate();
            let endpoint_a = Endpoint::builder(presets::Minimal)
                .secret_key(secret_key_a.clone())
                .address_lookup(lookup.clone())
                .bind()
                .await
                .expect("bind source endpoint");
            let endpoint_b = Endpoint::builder(presets::Minimal)
                .secret_key(secret_key_b.clone())
                .address_lookup(lookup.clone())
                .bind()
                .await
                .expect("bind receiver endpoint");
            lookup.add_endpoint_info(endpoint_a.addr());
            lookup.add_endpoint_info(endpoint_b.addr());
            let id_a = endpoint_a.id();
            let id_b = endpoint_b.id();
            let device_a = devices
                .create(
                    "owner".into(),
                    "source".into(),
                    id_a.to_string(),
                    "source-address".into(),
                    vec![],
                    0,
                )
                .await
                .expect("create source device");
            let pending_b = devices
                .create_pairing(
                    "owner".into(),
                    id_b.to_string(),
                    "receiver-address".into(),
                    id_a.to_string(),
                )
                .await
                .expect("create receiver device");
            let device_b = devices
                .approve_pairing(pending_b.id)
                .await
                .expect("approve receiver device")
                .expect("receiver device exists");
            let application_id = idp_model::model::Id::now_v7();
            let namespace = Namespace {
                owner_subject: "owner".into(),
                application_id,
            };
            let source_runtime = Arc::new(
                ScopedFileSystemRuntime::new(root.join("source"), id_a)
                    .expect("create source filesystem runtime"),
            );
            let receiver_runtime = Arc::new(
                ScopedFileSystemRuntime::new(root.join("receiver"), id_b)
                    .expect("create receiver filesystem runtime"),
            );
            let source_databases = Arc::new(
                DatabaseRuntime::new(root.join("source-databases"))
                    .expect("create source database runtime"),
            );
            let receiver_databases = Arc::new(
                DatabaseRuntime::new(root.join("receiver-databases"))
                    .expect("create receiver database runtime"),
            );
            let (database_resource, source_database) = source_databases
                .create(&namespace, Some("shared".into()))
                .expect("create database in the filesystem namespace");
            source_database
                .translate_and_execute("CREATE TABLE records (id UUID PRIMARY KEY)", &SqlTranslator)
                .await
                .expect("create database schema");
            let resource = source_runtime
                .create_resource(&namespace, Some("shared".into()))
                .await
                .expect("create source filesystem");
            let source_fs = source_runtime
                .open_resource(&namespace, resource.id)
                .await
                .expect("open source filesystem");
            source_fs
                .write("selected.txt", b"selected bytes")
                .await
                .expect("write selected content");
            let second_resource = source_runtime
                .create_resource(&namespace, Some("shared".into()))
                .await
                .expect("create second source filesystem with duplicate name");
            let second_source_fs = source_runtime
                .open_resource(&namespace, second_resource.id)
                .await
                .expect("open second source filesystem");
            second_source_fs
                .write("second.txt", b"second resource bytes")
                .await
                .expect("write second resource content");
            for device in [&device_a, &device_b] {
                policies
                    .set_prevalidated(SelectionPolicy {
                        device_id: device.id,
                        owner_subject: "owner".into(),
                        application_id: Some(application_id),
                        selected_kind: Some("filesystem".into()),
                        selected_id: Some(resource.id.as_uuid()),
                        admin_allowed: true,
                    })
                    .await
                    .expect("select filesystem on both devices");
            }
            let allowed_a = EndpointIdStore::new();
            allowed_a.replace([id_b]);
            let allowed_b = EndpointIdStore::new();
            allowed_b.replace([id_a]);
            let server_a = Server::new(endpoint_a.clone(), allowed_a);
            let server_b = Server::new(endpoint_b.clone(), allowed_b);
            let identity_a = Arc::new(DeviceIdentity::new(endpoint_a, secret_key_a));
            let identity_b = Arc::new(DeviceIdentity::new(endpoint_b, secret_key_b));
            let handler_a = StorageProtocolHandler::new(
                Arc::clone(&identity_a),
                Arc::clone(&devices),
                Arc::clone(&policies),
                Arc::clone(&source_runtime),
            );
            let handler_b = StorageProtocolHandler::new(
                Arc::clone(&identity_b),
                Arc::clone(&devices),
                Arc::clone(&policies),
                Arc::clone(&receiver_runtime),
            );
            for device in [&device_a, &device_b] {
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
                    .expect("begin with no selected resources");
            }
            let descriptor = super::descriptor(&management_service::replica::SelectedResource {
                device_id: device_a.id,
                owner_subject: "owner".into(),
                application_id,
                kind: "filesystem".into(),
                resource_id: resource.id.as_uuid(),
            });
            let second_descriptor =
                super::descriptor(&management_service::replica::SelectedResource {
                    device_id: device_a.id,
                    owner_subject: "owner".into(),
                    application_id,
                    kind: "filesystem".into(),
                    resource_id: second_resource.id.as_uuid(),
                });
            assert!(
                !handler_a
                    .authorize(&descriptor, &id_a.to_string(), &id_b.to_string())
                    .await,
                "paired but unselected peers cannot start filesystem sync"
            );
            for device in [&device_a, &device_b] {
                policies
                    .set_prevalidated(SelectionPolicy {
                        device_id: device.id,
                        owner_subject: "owner".into(),
                        application_id: Some(application_id),
                        selected_kind: Some("filesystem".into()),
                        selected_id: Some(resource.id.as_uuid()),
                        admin_allowed: true,
                    })
                    .await
                    .expect("select first filesystem for the remaining checks");
                policies
                    .set_prevalidated(SelectionPolicy {
                        device_id: device.id,
                        owner_subject: "owner".into(),
                        application_id: Some(application_id),
                        selected_kind: Some("filesystem".into()),
                        selected_id: Some(second_resource.id.as_uuid()),
                        admin_allowed: true,
                    })
                    .await
                    .expect("select second filesystem independently");
                policies
                    .set_prevalidated(SelectionPolicy {
                        device_id: device.id,
                        owner_subject: "owner".into(),
                        application_id: Some(application_id),
                        selected_kind: Some("database".into()),
                        selected_id: Some(database_resource.id),
                        admin_allowed: true,
                    })
                    .await
                    .expect("select database in the same namespace");
            }
            assert!(
                handler_a
                    .authorize(&second_descriptor, &id_a.to_string(), &id_b.to_string())
                    .await,
                "a second selected filesystem has independent authorization"
            );
            let database_handler_a = DatabaseProtocolHandler::new(
                Arc::clone(&identity_a),
                Arc::clone(&devices),
                Arc::clone(&policies),
                Arc::clone(&source_databases),
            );
            let database_handler_b = DatabaseProtocolHandler::new(
                Arc::clone(&identity_b),
                Arc::clone(&devices),
                Arc::clone(&policies),
                Arc::clone(&receiver_databases),
            );
            let no_peer_server = Server::new(identity_a.endpoint().clone(), EndpointIdStore::new());
            database_handler_a
                .synchronize_selected_peers(&no_peer_server)
                .await;
            handler_a.synchronize_selected_peers(&no_peer_server).await;
            let router_a = server_a.router(DataProtocolHandler::new(
                database_handler_a.clone(),
                handler_a.clone(),
            ));
            let router_b = server_b.router(DataProtocolHandler::new(database_handler_b, handler_b));
            let database_descriptor = crate::database_protocol::DatabaseResourceDescriptor {
                owner_subject: "owner".into(),
                application_id: application_id.to_string(),
                database_id: database_resource.id.to_string(),
            };
            assert_eq!(
                database_handler_a
                    .synchronize_peer(
                        &server_a,
                        id_b,
                        crate::database_protocol::DatabaseResourceDescriptor {
                            database_id: resource.id.as_uuid().to_string(),
                            ..database_descriptor.clone()
                        },
                    )
                    .await
                    .expect_err("filesystem ID cannot authorize database sync")
                    .kind(),
                std::io::ErrorKind::PermissionDenied,
                "cross-kind selection is denied in the shared namespace"
            );
            for wrong_namespace in [
                crate::database_protocol::DatabaseResourceDescriptor {
                    owner_subject: "other-owner".into(),
                    ..database_descriptor.clone()
                },
                crate::database_protocol::DatabaseResourceDescriptor {
                    application_id: idp_model::model::Id::now_v7().to_string(),
                    ..database_descriptor.clone()
                },
            ] {
                assert_eq!(
                    database_handler_a
                        .synchronize_peer(&server_a, id_b, wrong_namespace)
                        .await
                        .expect_err("database access outside the selected namespace is denied")
                        .kind(),
                    std::io::ErrorKind::PermissionDenied
                );
            }
            assert!(
                !handler_a
                    .authorize(
                        &super::IrohResourceDescriptor {
                            filesystem_id: database_resource.id.to_string(),
                            ..descriptor.clone()
                        },
                        &id_a.to_string(),
                        &id_b.to_string(),
                    )
                    .await,
                "database ID cannot authorize filesystem sync in the shared namespace"
            );
            database_handler_a
                .synchronize_peer(&server_a, id_b, database_descriptor)
                .await
                .expect("database syncs beside filesystems in the same namespace");
            assert_eq!(
                receiver_databases
                    .open_selected(&namespace, database_resource.id)
                    .expect("open matching selected database")
                    .expect("database provisions on the receiver")
                    .table_schema("records")
                    .await
                    .expect("database schema synchronizes")
                    .name,
                "records"
            );
            send_raw_filesystem_frame(
                &server_a,
                id_b,
                (file_system::MAX_FILESYSTEM_SYNC_FRAME_SIZE + 1) as u32,
                &[],
            )
            .await;
            send_raw_filesystem_frame(&server_a, id_b, 1, b"{").await;
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert!(
                receiver_runtime
                    .list_resources(&namespace)
                    .await
                    .expect("malformed handshakes leave the catalog readable")
                    .is_empty(),
                "oversized and malformed frames must not provision a filesystem"
            );
            let wrong_namespace = super::IrohResourceDescriptor {
                application_id: idp_model::model::Id::now_v7().to_string(),
                ..descriptor.clone()
            };
            assert!(
                !handler_a
                    .authorize(&wrong_namespace, &id_a.to_string(), &id_b.to_string())
                    .await,
                "selected filesystem cannot be authorized in another application namespace"
            );
            for device in [&device_a, &device_b] {
                policies
                    .deselect_resource_owned(
                        device.id,
                        "owner",
                        application_id,
                        "filesystem",
                        resource.id.as_uuid(),
                    )
                    .await
                    .expect("deselect filesystem resource");
                policies
                    .set_prevalidated(SelectionPolicy {
                        device_id: device.id,
                        owner_subject: "owner".into(),
                        application_id: Some(application_id),
                        selected_kind: Some("database".into()),
                        selected_id: Some(resource.id.as_uuid()),
                        admin_allowed: true,
                    })
                    .await
                    .expect("select mismatched resource kind");
            }
            assert!(
                !handler_a
                    .authorize(&descriptor, &id_a.to_string(), &id_b.to_string())
                    .await,
                "database selection cannot authorize filesystem sync"
            );
            for device in [&device_a, &device_b] {
                policies
                    .set_prevalidated(SelectionPolicy {
                        device_id: device.id,
                        owner_subject: "owner".into(),
                        application_id: Some(application_id),
                        selected_kind: Some("filesystem".into()),
                        selected_id: Some(resource.id.as_uuid()),
                        admin_allowed: true,
                    })
                    .await
                    .expect("restore filesystem selection");
            }
            assert!(
                handler_a
                    .authorize(&descriptor, &id_a.to_string(), &id_b.to_string())
                    .await,
                "a current matching selection permits a fresh authorized session"
            );
            assert!(
                handler_a
                    .authorize(&second_descriptor, &id_a.to_string(), &id_b.to_string())
                    .await,
                "selecting another filesystem does not deselect the first"
            );
            let outbound_handler = handler_a.clone();
            let outbound_manager = server_a.clone();
            let outbound_descriptor = descriptor.clone();
            let sync = tokio::spawn(async move {
                outbound_handler
                    .synchronize_peer(&outbound_manager, id_b, outbound_descriptor)
                    .await
            });
            tokio::time::timeout(Duration::from_secs(60), async {
                loop {
                    let resources = receiver_runtime
                        .list_resources(&namespace)
                        .await
                        .expect("read receiver selected catalog");
                    if resources.iter().any(|item| item.id == resource.id) {
                        let filesystem = receiver_runtime
                            .open_resource(&namespace, resource.id)
                            .await
                            .expect("open projected filesystem");
                        if filesystem.entry("selected.txt").await.is_ok() {
                            break;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("authorized filesystem metadata replicates");

            let second_sync_handler = handler_a.clone();
            let second_sync_manager = server_a.clone();
            let second_sync_descriptor = second_descriptor.clone();
            let second_sync = tokio::spawn(async move {
                second_sync_handler
                    .synchronize_peer(&second_sync_manager, id_b, second_sync_descriptor)
                    .await
            });
            tokio::time::timeout(Duration::from_secs(60), async {
                loop {
                    if receiver_runtime
                        .list_resources(&namespace)
                        .await
                        .expect("list independently synchronized filesystems")
                        .iter()
                        .any(|item| item.id == second_resource.id)
                    {
                        let filesystem = receiver_runtime
                            .open_resource(&namespace, second_resource.id)
                            .await
                            .expect("open second projected filesystem");
                        if filesystem.entry("second.txt").await.is_ok() {
                            break;
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("second selected filesystem synchronizes independently");

            for device in [&device_a, &device_b] {
                assert!(
                    policies
                        .deselect_resource_owned(
                            device.id,
                            "owner",
                            application_id,
                            "filesystem",
                            resource.id.as_uuid(),
                        )
                        .await
                        .expect("deselect only first filesystem"),
                    "first filesystem selection exists"
                );
            }
            assert!(
                !handler_a
                    .authorize(&descriptor, &id_a.to_string(), &id_b.to_string())
                    .await,
                "deselecting one filesystem blocks its sync"
            );
            assert!(
                handler_a
                    .authorize(&second_descriptor, &id_a.to_string(), &id_b.to_string())
                    .await,
                "deselecting one filesystem preserves the other selection"
            );

            for device in [&device_a, &device_b] {
                assert!(
                    policies
                        .deselect_resource_owned(
                            device.id,
                            "owner",
                            application_id,
                            "filesystem",
                            second_resource.id.as_uuid(),
                        )
                        .await
                        .expect("deselect second filesystem after replication"),
                    "second filesystem selection exists"
                );
            }
            assert!(
                !handler_a
                    .authorize(&descriptor, &id_a.to_string(), &id_b.to_string())
                    .await,
                "a reconnected session is denied while the latest policy is deselected"
            );
            source_fs
                .write("after-deselect.txt", b"must not replicate")
                .await
                .expect("write after deselection");
            let _sync_result = tokio::time::timeout(Duration::from_secs(5), sync)
                .await
                .expect("sync stops after policy revocation")
                .expect("sync task joins");
            let _second_sync_result = tokio::time::timeout(Duration::from_secs(5), second_sync)
                .await
                .expect("second sync stops after policy revocation")
                .expect("second sync task joins");
            let receiver_fs = receiver_runtime
                .open_resource(&namespace, resource.id)
                .await
                .expect("receiver filesystem remains catalogued");
            assert!(receiver_fs.entry("after-deselect.txt").await.is_err());
            let receiver_root = root
                .join("receiver")
                .join("filesystems")
                .join("owner")
                .join(application_id.to_string())
                .join("filesystems")
                .join(resource.id.as_uuid().to_string());
            drop(receiver_fs);
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    receiver_runtime
                        .remove_unselected_projected_resources(&[])
                        .await
                        .expect("evict deselected projected copy");
                    if !receiver_root.exists() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("deselection removes local copy after handles close");
            assert!(
                receiver_runtime
                    .list_resources(&namespace)
                    .await
                    .expect("resource identity remains catalogued")
                    .iter()
                    .any(|item| item.id == resource.id),
                "local deselection does not create a deletion tombstone"
            );
            for device in [&device_a, &device_b] {
                policies
                    .set_prevalidated(SelectionPolicy {
                        device_id: device.id,
                        owner_subject: "owner".into(),
                        application_id: Some(application_id),
                        selected_kind: Some("filesystem".into()),
                        selected_id: Some(resource.id.as_uuid()),
                        admin_allowed: true,
                    })
                    .await
                    .expect("causally later selection restores sync");
            }
            let receiver_fs = receiver_runtime
                .open_resource(&namespace, resource.id)
                .await
                .expect("reselected filesystem opens with a clean local copy");
            assert!(
                receiver_fs.entry("selected.txt").await.is_err(),
                "deselected local metadata is not restored from a global tombstone"
            );
            let reconnect_handler = handler_a.clone();
            let reconnect_manager = server_a.clone();
            let reconnect_descriptor = descriptor.clone();
            let reconnect = tokio::spawn(async move {
                reconnect_handler
                    .synchronize_peer(&reconnect_manager, id_b, reconnect_descriptor)
                    .await
            });
            tokio::time::timeout(Duration::from_secs(60), async {
                loop {
                    if receiver_fs.entry("after-deselect.txt").await.is_ok() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("later selection permits a fresh session to reconcile missed metadata");
            reconnect.abort();
            let _ = reconnect.await;

            source_runtime
                .delete_resource(&namespace, resource.id)
                .await
                .expect("owner deletes first filesystem");
            handler_a
                .synchronize_peer(&server_a, id_b, descriptor.clone())
                .await
                .expect("replicate filesystem deletion tombstone");
            tokio::time::timeout(Duration::from_secs(10), async {
                loop {
                    let resources = receiver_runtime
                        .list_resources(&namespace)
                        .await
                        .expect("receiver catalog remains readable");
                    assert!(
                        resources.iter().any(|item| item.id == second_resource.id),
                        "deleting one filesystem preserves the other"
                    );
                    if resources.iter().all(|item| item.id != resource.id) {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("filesystem tombstone reaches selected receiver");

            server_a.close().await;
            server_b.close().await;
            drop((router_a, router_b));
            drop((
                source_runtime,
                receiver_runtime,
                source_fs,
                receiver_fs,
                second_source_fs,
                handler_a,
                server_a,
                server_b,
            ));
            let reopen_started = Instant::now();
            let restarted_receiver = loop {
                match ScopedFileSystemRuntime::new(root.join("receiver"), id_b) {
                    Ok(runtime) => break runtime,
                    Err(error)
                        if error.to_string().contains("Database already open")
                            && reopen_started.elapsed() < Duration::from_secs(5) =>
                    {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    Err(error) => panic!("reopen receiver filesystem runtime: {error}"),
                }
            };
            let resources = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    match restarted_receiver.list_resources(&namespace).await {
                        Ok(resources) => break resources,
                        Err(error) if error.contains("Database already open") => {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                        Err(error) => panic!("read receiver filesystem catalog: {error}"),
                    }
                }
            })
            .await
            .expect("receiver filesystem catalog lock is released after stream shutdown");
            assert!(
                resources.iter().all(|item| item.id != resource.id),
                "replicated deletion tombstone survives receiver restart"
            );
            assert!(
                resources.iter().any(|item| item.id == second_resource.id),
                "another filesystem remains available after receiver restart"
            );
            drop(restarted_receiver);
            fs::remove_dir_all(root).expect("remove test root");
        })
        .await
        .expect("selected filesystem integration test completes");
    }
}
