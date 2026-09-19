use std::sync::Arc;

use crate::{ManagementError, ManagementResult};
use chrono::Utc;
use file_system::{FileSystem, PeerCodec, Storage, Transport};
use idp_model::{
    contract::{DeviceState, TrustedDevice},
    model::Device,
};
use serde::{Deserialize, Serialize};

use crate::DeviceRepo;

use super::json_store::JsonStore;

const DEVICES_FOLDER: &str = "idp/devices";

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DeviceRecord {
    device: Device,
    enrollment_code_hash: Option<Vec<u8>>,
    enrollment_expires_at: Option<i64>,
    pairing_accepting_public_key: Option<String>,
}

pub struct FsDeviceRepo<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>> {
    store: JsonStore<S, C, T>,
}

impl<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>> Clone for FsDeviceRepo<S, C, T> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
        }
    }
}

impl<S, C, T> FsDeviceRepo<S, C, T>
where
    S: Storage + Send + Sync + 'static,
    S::Error: std::fmt::Display + Send + 'static,
    C: PeerCodec + Send + 'static,
    C::Error: Send + 'static,
    C::PeerId: Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: 'static,
{
    #[must_use]
    pub fn new(file_system: Arc<FileSystem<S, C, T>>) -> Self {
        Self {
            store: JsonStore::new(file_system),
        }
    }

    async fn records(&self) -> ManagementResult<Vec<DeviceRecord>> {
        let paths = self.store.list(DEVICES_FOLDER).await?;
        let mut records = Vec::with_capacity(paths.len());
        for path in paths {
            records.push(self.store.read(&path).await?);
        }
        records.sort_by_key(|record: &DeviceRecord| record.device.id);
        Ok(records)
    }

    async fn record(&self, id: idp_model::model::Id) -> ManagementResult<Option<DeviceRecord>> {
        match self.store.read(&path(id)).await {
            Ok(record) => Ok(Some(record)),
            Err(ManagementError::InvalidInput(message)) if message.contains("not found") => {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    async fn write(&self, record: &DeviceRecord) -> ManagementResult<()> {
        self.store.write(&path(record.device.id), record).await
    }

    async fn next_id(&self) -> ManagementResult<idp_model::model::Id> {
        // ponytail: scan records; replace with a replicated ID allocator only if device creation is hot.
        let ids = self
            .records()
            .await?
            .into_iter()
            .map(|record| record.device.id)
            .collect::<std::collections::BTreeSet<_>>();
        loop {
            let mut bytes = [0_u8; 16];
            getrandom::fill(&mut bytes).map_err(ManagementError::other)?;
            let id = idp_model::model::Id::from_bytes(bytes);
            if !ids.contains(&id) {
                return Ok(id);
            }
        }
    }
}

impl<S, C, T> DeviceRepo for FsDeviceRepo<S, C, T>
where
    S: Storage + Send + Sync + 'static,
    S::Error: std::fmt::Display + Send + 'static,
    C: PeerCodec + Send + 'static,
    C::Error: Send + 'static,
    C::PeerId: Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: 'static,
{
    async fn create(
        &self,
        name: String,
        public_key: String,
        address: String,
        enrollment_code_hash: Vec<u8>,
        enrollment_expires_at: i64,
    ) -> ManagementResult<Device> {
        let now = Utc::now();
        let state = if self.has_any().await? {
            DeviceState::Pending
        } else {
            DeviceState::Approved
        };
        let record = DeviceRecord {
            device: Device {
                id: self.next_id().await?,
                name,
                public_key,
                address,
                state,
                created_at: now,
                updated_at: now,
                revoked_at: None,
            },
            enrollment_code_hash: (state == DeviceState::Pending).then_some(enrollment_code_hash),
            enrollment_expires_at: (state == DeviceState::Pending).then_some(enrollment_expires_at),
            pairing_accepting_public_key: None,
        };
        self.write(&record).await?;
        Ok(record.device)
    }

    async fn create_pairing(
        &self,
        name: String,
        public_key: String,
        address: String,
        accepting_public_key: String,
    ) -> ManagementResult<Device> {
        let now = Utc::now();
        let record = DeviceRecord {
            device: Device {
                id: self.next_id().await?,
                name,
                public_key,
                address,
                state: DeviceState::Pending,
                created_at: now,
                updated_at: now,
                revoked_at: None,
            },
            enrollment_code_hash: None,
            enrollment_expires_at: None,
            pairing_accepting_public_key: Some(accepting_public_key),
        };
        self.write(&record).await?;
        Ok(record.device)
    }

    async fn pending_pairing(
        &self,
        device_id: idp_model::model::Id,
    ) -> ManagementResult<Option<(Device, String)>> {
        Ok(self.record(device_id).await?.and_then(|record| {
            (record.device.state == DeviceState::Pending)
                .then_some(record.pairing_accepting_public_key)
                .flatten()
                .map(|key| (record.device, key))
        }))
    }

    async fn approve_pairing(
        &self,
        device_id: idp_model::model::Id,
    ) -> ManagementResult<Option<Device>> {
        let Some(mut record) = self.record(device_id).await? else {
            return Ok(None);
        };
        if record.device.state != DeviceState::Pending
            || record.pairing_accepting_public_key.is_none()
        {
            return Ok(None);
        }
        record.device.state = DeviceState::Approved;
        record.device.updated_at = Utc::now();
        record.pairing_accepting_public_key = None;
        self.write(&record).await?;
        Ok(Some(record.device))
    }

    async fn has_any(&self) -> ManagementResult<bool> {
        Ok(!self.records().await?.is_empty())
    }

    async fn list(&self) -> ManagementResult<Vec<Device>> {
        Ok(self
            .records()
            .await?
            .into_iter()
            .map(|record| record.device)
            .collect())
    }

    async fn list_approved(&self) -> ManagementResult<Vec<TrustedDevice>> {
        Ok(self
            .records()
            .await?
            .into_iter()
            .filter(|record| record.device.state == DeviceState::Approved)
            .map(|record| TrustedDevice {
                public_key: record.device.public_key,
                address: record.device.address,
            })
            .collect())
    }

    async fn are_approved(
        &self,
        local_public_key: &str,
        remote_public_key: &str,
    ) -> ManagementResult<bool> {
        let approved = self
            .records()
            .await?
            .into_iter()
            .filter(|record| record.device.state == DeviceState::Approved)
            .map(|record| record.device.public_key)
            .collect::<std::collections::BTreeSet<_>>();
        Ok(approved.contains(local_public_key) && approved.contains(remote_public_key))
    }

    async fn approve(
        &self,
        device_id: idp_model::model::Id,
        enrollment_code_hash: &[u8],
    ) -> ManagementResult<Option<Device>> {
        let Some(mut record) = self.record(device_id).await? else {
            return Ok(None);
        };
        let now = Utc::now().timestamp();
        if record.device.state != DeviceState::Pending
            || record.enrollment_code_hash.as_deref() != Some(enrollment_code_hash)
            || record
                .enrollment_expires_at
                .is_none_or(|expires_at| expires_at <= now)
        {
            return Ok(None);
        }
        record.device.state = DeviceState::Approved;
        record.device.updated_at = Utc::now();
        record.enrollment_code_hash = None;
        record.enrollment_expires_at = None;
        self.write(&record).await?;
        Ok(Some(record.device))
    }

    async fn rename(
        &self,
        device_id: idp_model::model::Id,
        name: String,
    ) -> ManagementResult<Option<Device>> {
        let Some(mut record) = self.record(device_id).await? else {
            return Ok(None);
        };
        if record.device.state == DeviceState::Revoked {
            return Ok(None);
        }
        record.device.name = name;
        record.device.updated_at = Utc::now();
        self.write(&record).await?;
        Ok(Some(record.device))
    }

    async fn revoke(
        &self,
        device_id: idp_model::model::Id,
        protected_public_key: &str,
    ) -> ManagementResult<bool> {
        let Some(mut record) = self.record(device_id).await? else {
            return Ok(false);
        };
        if record.device.public_key == protected_public_key
            || record.device.state == DeviceState::Revoked
        {
            return Ok(false);
        }
        let approved = self
            .records()
            .await?
            .into_iter()
            .filter(|other| other.device.state == DeviceState::Approved)
            .count();
        if record.device.state == DeviceState::Approved && approved <= 1 {
            return Ok(false);
        }
        record.device.state = DeviceState::Revoked;
        record.device.revoked_at = Some(Utc::now());
        record.device.updated_at = Utc::now();
        self.write(&record).await?;
        Ok(true)
    }

    async fn revoke_self(&self, public_key: &str) -> ManagementResult<bool> {
        let records = self.records().await?;
        let mut found = false;
        for mut record in records {
            if record.device.public_key != public_key {
                continue;
            }
            found = true;
            if record.device.state != DeviceState::Revoked {
                record.device.state = DeviceState::Revoked;
                record.device.revoked_at = Some(Utc::now());
                record.device.updated_at = Utc::now();
                self.write(&record).await?;
            }
        }
        Ok(found)
    }
}

fn path(id: idp_model::model::Id) -> String {
    format!("{DEVICES_FOLDER}/{id}.json")
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, sync::Arc};

    use file_system::{FileSystem, InMemoryStorage, MemoryTransport, PeerCodec};
    use idp_model::contract::DeviceState;

    use crate::DeviceRepo;

    use super::FsDeviceRepo;

    #[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
    struct Peer;

    impl PeerCodec for Peer {
        type Error = Infallible;
        type PeerId = Self;

        fn encode(_: &Self::PeerId) -> Vec<u8> {
            Vec::new()
        }

        fn decode(_: &[u8]) -> Result<Self::PeerId, Self::Error> {
            Ok(Self)
        }
    }

    #[tokio::test]
    async fn persists_pairing_approval() {
        let (transport, _) = MemoryTransport::pair(Peer, Peer);
        let file_system: Arc<FileSystem<InMemoryStorage, Peer, MemoryTransport<Peer>>> = Arc::new(
            FileSystem::new(InMemoryStorage::new(), Peer, transport)
                .await
                .unwrap(),
        );
        let repo = FsDeviceRepo::new(file_system);
        let device = repo
            .create_pairing(
                "device".into(),
                "device-key".into(),
                "device-address".into(),
                "accepting-key".into(),
            )
            .await
            .unwrap();

        assert!(repo.pending_pairing(device.id).await.unwrap().is_some());
        assert_eq!(
            repo.approve_pairing(device.id)
                .await
                .unwrap()
                .unwrap()
                .state,
            DeviceState::Approved
        );
        assert!(repo.pending_pairing(device.id).await.unwrap().is_none());
    }
}
