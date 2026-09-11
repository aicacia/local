use chrono::{DateTime, Utc};
use idp_model::{contract::EntityType, model::Key};
use key::DerivedKey;

use crate::repo::{KeyRepo, PrivateKeyKeyringRepo, PrivateKeyRepo, RepoResult};

pub struct KeyService<R> {
    key_repo: R,
    private_key_repo: PrivateKeyKeyringRepo,
    namespace: String,
}

impl<R> KeyService<R>
where
    R: KeyRepo,
{
    pub fn new(
        key_repo: R,
        private_key_repo: PrivateKeyKeyringRepo,
        namespace: impl Into<String>,
    ) -> Self {
        Self {
            key_repo,
            private_key_repo,
            namespace: namespace.into(),
        }
    }

    pub fn key_repo(&self) -> &R {
        &self.key_repo
    }

    pub fn private_key_repo(&self) -> &PrivateKeyKeyringRepo {
        &self.private_key_repo
    }

    pub async fn create_key(
        &self,
        parent_id: Option<u32>,
        entity_type: EntityType,
        entity_id: i64,
        hardened: bool,
        name: String,
        expires_at: Option<DateTime<Utc>>,
    ) -> RepoResult<(Key, DerivedKey)> {
        let key = self
            .key_repo
            .create_key(
                parent_id,
                entity_type,
                entity_id,
                hardened,
                name,
                expires_at,
            )
            .await?;

        let scoped_namespace = self.scoped_namespace(entity_type, entity_id);

        let private_key = self
            .private_key_repo
            .ensure_derivation_path(&scoped_namespace, key.derivation_path()?)?;

        Ok((key, private_key))
    }

    pub fn scoped_namespace(&self, entity_type: EntityType, entity_id: i64) -> String {
        format!("{}:{entity_type}:{entity_id}", self.namespace)
    }

    pub fn ensure_entity_master_key(
        &self,
        entity_type: EntityType,
        entity_id: i64,
        passphrase: &str,
    ) -> RepoResult<DerivedKey> {
        let scoped_namespace = self.scoped_namespace(entity_type, entity_id);
        self.private_key_repo
            .ensure_master_key_with_passphrase(&scoped_namespace, passphrase)
    }

    pub async fn rotate_active_entity_root_key(
        &self,
        entity_type: EntityType,
        entity_id: i64,
        name: String,
        expires_at: Option<DateTime<Utc>>,
    ) -> RepoResult<(Key, DerivedKey)> {
        self.create_key(None, entity_type, entity_id, true, name, expires_at)
            .await
    }
}
