use chrono::{DateTime, Utc};
use idp_model::{contract::EntityType, model::Key};

use crate::repo::RepoResult;

pub trait KeyRepo {
    fn list_active(&self) -> impl Future<Output = RepoResult<Vec<Key>>>;

    fn list_by_entity_type_and_id(
        &self,
        entity_type: EntityType,
        entity_id: i64,
    ) -> impl Future<Output = RepoResult<Vec<Key>>>;

    fn find_by_id(&self, id: u32) -> impl Future<Output = RepoResult<Option<Key>>> + Send;

    fn find_by_entity_type_and_id(
        &self,
        entity_type: EntityType,
        entity_id: i64,
    ) -> impl Future<Output = RepoResult<Option<Key>>>;

    fn find_active_entity_root_key(
        &self,
        entity_type: EntityType,
        entity_id: i64,
    ) -> impl Future<Output = RepoResult<Option<Key>>> + Send;

    fn create_key(
        &self,
        parent_id: Option<u32>,
        entity_type: EntityType,
        entity_id: i64,
        hardened: bool,
        name: String,
        expires_at: Option<DateTime<Utc>>,
    ) -> impl Future<Output = RepoResult<Key>>;
}
