use core::error::Error;

use chrono::{DateTime, Utc};
use futures_core::Stream;

use crate::lww_record::LwwRecord;

/// Pluggable durable (or in-memory) backend.
pub trait Storage<Id, K, V> {
    type Error: Error;

    fn insert(
        &self,
        key: &K,
        record: &LwwRecord<Id, V>,
    ) -> impl Future<Output = Result<(), Self::Error>>;

    fn get(&self, key: &K) -> impl Future<Output = Result<Option<LwwRecord<Id, V>>, Self::Error>>;

    fn remove(&self, key: &K) -> impl Future<Output = Result<(), Self::Error>>;

    fn get_latest_for_node_id(
        &self,
        node_id: Id,
    ) -> impl Future<Output = Result<Option<DateTime<Utc>>, Self::Error>>;

    fn stream(
        &self,
        node_id: Id,
        since: Option<DateTime<Utc>>,
    ) -> impl Stream<Item = Result<(K, LwwRecord<Id, V>), Self::Error>>;
}
