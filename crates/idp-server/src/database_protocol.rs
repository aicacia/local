use std::{future::Future, io, pin::Pin, sync::Arc};

use iroh::{
    EndpointId,
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use iroh_chain::{DATABASE_ALPN, Server};
use management_service::{
    DeviceRepo,
    replica::{DbDeviceRepo, DbSelectionPolicyRepo, SelectedResource},
};
use ofdb::{AutomergeRowCodec, IrohTransport, RedbKernel, SessionConfig, SyncRole, SyncTransport};
use serde::{Deserialize, Serialize};
use storage_model::StorageNamespace;
use storage_service::{DatabaseId, DatabaseRuntime};

use crate::DeviceIdentity;

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
            .connect_direct_with_alpn(peer, DATABASE_ALPN)
            .await
            .map_err(io::Error::other)?;
        let (mut send, mut recv) = connection.open_bi().await.map_err(io::Error::other)?;
        write_frame(
            &mut send,
            &serde_json::to_vec(&resource).map_err(io::Error::other)?,
        )
        .await?;
        if read_frame(&mut recv, MAX_HANDSHAKE_BYTES).await? != b"OK" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid database sync acknowledgement",
            ));
        }
        let database_id = resource
            .database_id
            .parse::<DatabaseId>()
            .map_err(io::Error::other)?;
        let application_id = resource.application_id.parse().map_err(io::Error::other)?;
        let namespace = Namespace {
            owner_subject: resource.owner_subject.clone(),
            application_id,
        };
        let database = self
            .databases
            .open(&namespace, database_id)
            .map_err(io::Error::other)?
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "database resource not found")
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
        if !self
            .databases
            .get(&namespace, database_id)
            .is_ok_and(|resource| resource.is_some())
        {
            return false;
        }
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

impl ProtocolHandler for DatabaseProtocolHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote_public_key = connection.remote_id().to_string();
        let local_public_key = self.identity.endpoint_id().to_string();
        loop {
            let (send, mut recv) = connection.accept_bi().await?;
            let resource = match read_descriptor(&mut recv).await {
                Ok(resource) => resource,
                Err(error) => {
                    log::warn!("rejected database sync handshake: {error}");
                    continue;
                }
            };
            if !self
                .authorize(&resource, &local_public_key, &remote_public_key)
                .await
            {
                log::warn!("rejected unauthorized database sync stream");
                continue;
            }
            let (Ok(application_id), Ok(database_id)) = (
                resource.application_id.parse(),
                resource.database_id.parse::<DatabaseId>(),
            ) else {
                continue;
            };
            let namespace = Namespace {
                owner_subject: resource.owner_subject.clone(),
                application_id,
            };
            let Some(database) = self.databases.open(&namespace, database_id).ok().flatten() else {
                continue;
            };
            let mut send = send;
            if let Err(error) = write_frame(&mut send, b"OK").await {
                log::warn!("failed to acknowledge database sync stream: {error}");
                continue;
            }
            let handler = self.clone();
            let local_public_key = local_public_key.clone();
            let remote_public_key = remote_public_key.clone();
            tokio::spawn(async move {
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
                if let Err(error) = database
                    .synchronize(
                        &mut transport,
                        &SessionConfig::default(),
                        SyncRole::Responder,
                    )
                    .await
                {
                    log::warn!("database sync session ended: {error}");
                }
            });
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

async fn read_descriptor(
    recv: &mut iroh::endpoint::RecvStream,
) -> io::Result<DatabaseResourceDescriptor> {
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
