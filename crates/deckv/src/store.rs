use chrono::{DateTime, Utc};
use core::hash::Hash;
use futures_util::{StreamExt, pin_mut};
use hashbrown::HashMap;
use tokio::sync::broadcast::{Receiver, Sender};

use crate::{lww_record::LwwRecord, storage::Storage};

pub struct Store<Id, K, V, S>
where
    S: Storage<Id, K, V>,
{
    node_id: Id,
    storage: S,
    sender: Sender<(K, LwwRecord<Id, V>)>,
}

impl<Id, K, V, S> Store<Id, K, V, S>
where
    Id: PartialOrd + Clone,
    K: Eq + Hash,
    S: Storage<Id, K, V>,
{
    pub fn new(node_id: Id, storage: S, sender: Sender<(K, LwwRecord<Id, V>)>) -> Self {
        Self {
            node_id,
            storage,
            sender,
        }
    }

    pub fn subscribe(&self) -> Receiver<(K, LwwRecord<Id, V>)> {
        self.sender.subscribe()
    }

    pub async fn insert(&self, key: K, value: V, timestamp: DateTime<Utc>) -> Result<(), S::Error> {
        let record = LwwRecord {
            value: Some(value),
            timestamp,
            node_id: self.node_id.clone(),
        };
        self.storage.insert(&key, &record).await?;
        let _ = self.sender.send((key, record));
        Ok(())
    }

    pub async fn delete(&self, key: K, timestamp: DateTime<Utc>) -> Result<(), S::Error> {
        let record = LwwRecord {
            value: None,
            timestamp,
            node_id: self.node_id.clone(),
        };
        self.storage.insert(&key, &record).await?;
        let _ = self.sender.send((key, record));
        Ok(())
    }

    pub async fn get(&self, key: &K) -> Result<Option<V>, S::Error> {
        match self.storage.get(key).await? {
            Some(rec) => Ok(rec.value),
            None => Ok(None),
        }
    }

    pub async fn get_latest_for_node_id(
        &self,
        node_id: Id,
    ) -> Result<Option<DateTime<Utc>>, S::Error> {
        self.storage.get_latest_for_node_id(node_id).await
    }

    pub async fn export_for(
        &self,
        peer: Id,
        since: Option<DateTime<Utc>>,
    ) -> Result<HashMap<K, LwwRecord<Id, V>>, S::Error> {
        let mut delta = HashMap::new();
        let stream = self.storage.stream(peer, since);
        pin_mut!(stream);

        while let Some(item) = stream.next().await {
            let (key, rec) = item?;
            delta.insert(key, rec);
        }

        Ok(delta)
    }

    pub async fn export(
        &self,
        since: Option<DateTime<Utc>>,
    ) -> Result<HashMap<K, LwwRecord<Id, V>>, S::Error> {
        self.export_for(self.node_id.clone(), since).await
    }

    pub async fn merge_peer_state(
        &self,
        remote: HashMap<K, LwwRecord<Id, V>>,
    ) -> Result<(), S::Error> {
        for (key, remote_rec) in remote {
            let take = match self.storage.get(&key).await? {
                Some(local_rec) => local_rec.should_take(&remote_rec),
                None => true,
            };

            if take {
                self.storage.insert(&key, &remote_rec).await?;
            }
        }

        Ok(())
    }
}
