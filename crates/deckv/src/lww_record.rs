use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LwwRecord<Id, V> {
    /// The value of the record. If `None`, it represents a tombstone (deletion).
    pub value: Option<V>,
    pub timestamp: DateTime<Utc>,
    pub node_id: Id,
}

impl<Id, V> LwwRecord<Id, V> {
    pub fn new(value: Option<V>, timestamp: DateTime<Utc>, node_id: Id) -> Self {
        Self {
            value,
            timestamp,
            node_id,
        }
    }
}

impl<Id, V> LwwRecord<Id, V>
where
    Id: PartialOrd,
{
    pub fn should_take(&self, other: &Self) -> bool {
        self.timestamp > other.timestamp
            || (self.timestamp == other.timestamp && self.node_id > other.node_id)
    }
}
