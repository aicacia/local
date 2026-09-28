use std::sync::Arc;

use file_system::{FileSystemId, IrohFileTransport, IrohResourceDescriptor};
use iroh::{
    EndpointId,
    endpoint::Connection,
    protocol::{AcceptError, ProtocolHandler},
};
use management_service::replica::{DbDeviceRepo, DbSelectionPolicyRepo};
use ofdb::{AutomergeRowCodec, RedbKernel};
use storage_model::{ResourceCatalog, ResourceIdentity, ResourceKind, StorageNamespace};
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
}

impl core::fmt::Debug for StorageProtocolHandler {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("StorageProtocolHandler")
            .finish_non_exhaustive()
    }
}

impl ProtocolHandler for StorageProtocolHandler {
    async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
        let remote_public_key = connection.remote_id().to_string();
        let local_public_key = self.identity.endpoint_id().to_string();
        loop {
            let (send, recv) = connection.accept_bi().await?;
            let local_public_key = local_public_key.clone();
            let remote_public_key = remote_public_key.clone();
            let handler = self.clone();
            let (transport, resource) = match IrohFileTransport::accept_authorized(
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
                    continue;
                }
            };

            let Ok(application_id) = resource.application_id.parse() else {
                continue;
            };
            let Ok(filesystem_id) = FileSystemId::parse(&resource.filesystem_id) else {
                continue;
            };
            let namespace = Namespace {
                owner_subject: resource.owner_subject,
                application_id,
            };
            let file_system = match self
                .file_systems
                .open_resource(&namespace, filesystem_id)
                .await
            {
                Ok(file_system) => file_system,
                Err(error) => {
                    log::warn!("failed to open selected filesystem: {error}");
                    continue;
                }
            };
            tokio::spawn(async move {
                if let Err(error) = file_system.sync_peer(transport).await {
                    log::warn!("filesystem sync session ended: {error}");
                }
            });
        }
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
        let identity = ResourceIdentity {
            kind: ResourceKind::FileSystem,
            id: filesystem_id.as_uuid().to_string(),
        };
        if !self
            .file_systems
            .contains(&namespace, &identity)
            .unwrap_or(false)
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
                "filesystem",
                filesystem_id.as_uuid(),
            )
            .await
            .unwrap_or(false)
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
