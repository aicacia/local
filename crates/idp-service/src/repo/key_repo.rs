use chrono::{DateTime, Utc};
use idp_model::{
    contract::EntityType,
    model::{Id, Key},
};

use crate::repo::RepoResult;

pub trait KeyRepo {
    fn list_active(&self) -> impl Future<Output = RepoResult<Vec<Key>>>;

    fn list_by_entity_type_and_id(
        &self,
        entity_type: EntityType,
        entity_id: Id,
    ) -> impl Future<Output = RepoResult<Vec<Key>>>;

    fn find_by_id(&self, id: Id) -> impl Future<Output = RepoResult<Option<Key>>>;

    fn find_by_entity_type_and_id(
        &self,
        entity_type: EntityType,
        entity_id: Id,
    ) -> impl Future<Output = RepoResult<Option<Key>>>;

    fn find_active_entity_root_key(
        &self,
        entity_type: EntityType,
        entity_id: Id,
    ) -> impl Future<Output = RepoResult<Option<Key>>>;

    fn create_key(
        &self,
        parent_id: Option<Id>,
        entity_type: EntityType,
        entity_id: Id,
        hardened: bool,
        name: String,
        expires_at: Option<DateTime<Utc>>,
    ) -> impl Future<Output = RepoResult<Key>>;
}
