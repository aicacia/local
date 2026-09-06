use std::{collections::BTreeSet, sync::Arc};

use iroh::EndpointId;
use tokio::sync::RwLock;

use crate::AllowedEndpointId;

#[derive(Clone, Default)]
pub struct DynamicEndpointIdStore {
    ids: Arc<RwLock<BTreeSet<EndpointId>>>,
}

impl DynamicEndpointIdStore {
    pub async fn replace(&self, ids: impl IntoIterator<Item = EndpointId>) {
        *self.ids.write().await = ids.into_iter().collect();
    }
}

impl AllowedEndpointId for DynamicEndpointIdStore {
    async fn allowed(&self, id: EndpointId) -> bool {
        self.ids.read().await.contains(&id)
    }
}

#[cfg(test)]
mod tests {
    use iroh::SecretKey;

    use super::{AllowedEndpointId, DynamicEndpointIdStore};

    #[tokio::test]
    async fn replaces_allowed_ids() {
        let first = SecretKey::generate().public();
        let second = SecretKey::generate().public();
        let store = DynamicEndpointIdStore::default();
        store.replace([first]).await;
        assert!(store.allowed(first).await);
        assert!(!store.allowed(second).await);
        store.replace([second]).await;
        assert!(!store.allowed(first).await);
        assert!(store.allowed(second).await);
    }
}
