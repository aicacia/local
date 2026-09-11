use std::{collections::BTreeSet, sync::Arc};

use crate::repo::{KeyRepo, RepoError, RepoResult};
use chrono::{DateTime, Utc};
use file_system::{FileSystem, PeerCodec, Storage, Transport};
use idp_model::{contract::EntityType, model::Key};

use super::JsonStore;

const KEYS_FOLDER: &str = "idp/keys";

pub struct FsKeyRepo<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>> {
    store: JsonStore<S, C, T>,
}

impl<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>> Clone for FsKeyRepo<S, C, T> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
        }
    }
}

impl<S, C, T> FsKeyRepo<S, C, T>
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

    async fn keys(&self) -> RepoResult<Vec<Key>> {
        let paths = self.store.list(KEYS_FOLDER).await?;
        let mut keys: Vec<Key> = Vec::with_capacity(paths.len());
        for path in paths {
            keys.push(self.store.read(&path).await?);
        }
        keys.sort_by_key(|key| key.created_at);
        Ok(keys)
    }

    async fn next_id(&self) -> RepoResult<u32> {
        // ponytail: scan records; replace with a replicated ID allocator only if key creation is hot.
        let ids = self
            .keys()
            .await?
            .into_iter()
            .map(|key| key.id)
            .collect::<BTreeSet<_>>();
        loop {
            let mut bytes = [0_u8; 4];
            getrandom::fill(&mut bytes).map_err(RepoError::other)?;
            let id = u32::from_be_bytes(bytes);
            if id != 0 && !ids.contains(&id) {
                return Ok(id);
            }
        }
    }
}

impl<S, C, T> KeyRepo for FsKeyRepo<S, C, T>
where
    S: Storage + Send + Sync + 'static,
    S::Error: std::fmt::Display + Send + 'static,
    C: PeerCodec + Send + 'static,
    C::Error: Send + 'static,
    C::PeerId: Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: 'static,
{
    async fn list_active(&self) -> RepoResult<Vec<Key>> {
        let now = Utc::now();
        Ok(self
            .keys()
            .await?
            .into_iter()
            .filter(|key| key.revoked_at.is_none_or(|date| date > now))
            .filter(|key| key.expires_at.is_none_or(|date| date > now))
            .collect())
    }

    async fn list_by_entity_type_and_id(
        &self,
        entity_type: EntityType,
        entity_id: i64,
    ) -> RepoResult<Vec<Key>> {
        Ok(self
            .list_active()
            .await?
            .into_iter()
            .filter(|key| key.entity_type == entity_type && key.entity_id == entity_id)
            .collect())
    }

    async fn find_by_id(&self, id: u32) -> RepoResult<Option<Key>> {
        match self.store.read(&path(id)).await {
            Ok(key) if is_active(&key) => Ok(Some(key)),
            Ok(_) => Ok(None),
            Err(RepoError::InvalidInput(message)) if message.contains("not found") => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn find_by_entity_type_and_id(
        &self,
        entity_type: EntityType,
        entity_id: i64,
    ) -> RepoResult<Option<Key>> {
        self.find_active_entity_root_key(entity_type, entity_id)
            .await
    }

    async fn find_active_entity_root_key(
        &self,
        entity_type: EntityType,
        entity_id: i64,
    ) -> RepoResult<Option<Key>> {
        Ok(self
            .list_by_entity_type_and_id(entity_type, entity_id)
            .await?
            .into_iter()
            .rev()
            .find(|key| key.parent_id.is_none()))
    }

    async fn create_key(
        &self,
        parent_id: Option<u32>,
        entity_type: EntityType,
        entity_id: i64,
        hardened: bool,
        name: String,
        expires_at: Option<DateTime<Utc>>,
    ) -> RepoResult<Key> {
        let parent_derivation_path = match parent_id {
            Some(parent_id) => self
                .find_by_id(parent_id)
                .await?
                .map(|key| key.derivation_path),
            None => None,
        };
        let id = self.next_id().await?;
        let now = Utc::now();
        let key = Key {
            id,
            parent_id,
            entity_type,
            entity_id,
            derivation_path: Key::build_derivation_path(
                parent_derivation_path.as_deref(),
                id,
                hardened,
            ),
            name,
            hardened,
            revoked_at: None,
            expires_at,
            created_at: now,
            updated_at: now,
        };
        self.store.write(&path(id), &key).await?;
        Ok(key)
    }
}

fn is_active(key: &Key) -> bool {
    let now = Utc::now();
    key.revoked_at.is_none_or(|date| date > now) && key.expires_at.is_none_or(|date| date > now)
}

fn path(id: u32) -> String {
    format!("{KEYS_FOLDER}/{id}.json")
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, sync::Arc};

    use crate::repo::KeyRepo;
    use file_system::{FileSystem, InMemoryStorage, MemoryTransport, PeerCodec};
    use idp_model::contract::EntityType;

    use super::FsKeyRepo;

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
    async fn persists_active_entity_keys() {
        let (transport, _) = MemoryTransport::pair(Peer, Peer);
        let file_system: Arc<FileSystem<InMemoryStorage, Peer, MemoryTransport<Peer>>> = Arc::new(
            FileSystem::new(InMemoryStorage::new(), Peer, transport)
                .await
                .unwrap(),
        );
        let repo = FsKeyRepo::new(file_system);
        let key = repo
            .create_key(None, EntityType::User, 1, true, "user".into(), None)
            .await
            .unwrap();

        assert_eq!(key.derivation_path, format!("m/{}'", key.id));
        assert_eq!(
            repo.find_active_entity_root_key(EntityType::User, 1)
                .await
                .unwrap()
                .unwrap()
                .id,
            key.id
        );
    }
}
