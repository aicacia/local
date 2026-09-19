use std::{future::Future, pin::Pin, sync::Arc};

use file_system::Transport;
use iroh::EndpointId;

use crate::{GlobalIdentityCache, GlobalIdentityRuntime};

pub trait GlobalIdentityReadGate: Send + Sync + 'static {
    fn verify(&self) -> Pin<Box<dyn Future<Output = bool> + Send + '_>>;
}

pub struct ActiveGlobalIdentityReadGate<T>
where
    T: Transport<EndpointId>,
{
    runtime: Arc<GlobalIdentityRuntime<T>>,
    cache: GlobalIdentityCache,
}

impl<T> ActiveGlobalIdentityReadGate<T>
where
    T: Transport<EndpointId>,
{
    #[must_use]
    pub fn new(runtime: Arc<GlobalIdentityRuntime<T>>, cache: GlobalIdentityCache) -> Self {
        Self { runtime, cache }
    }
}

impl<T> GlobalIdentityReadGate for ActiveGlobalIdentityReadGate<T>
where
    T: Transport<EndpointId> + Clone + Send + Sync + 'static,
    T::Error: std::fmt::Display,
{
    fn verify(&self) -> Pin<Box<dyn Future<Output = bool> + Send + '_>> {
        Box::pin(async move {
            let Ok(Some((manifest, _))) = self.runtime.active_rows().await else {
                return false;
            };
            matches!(self.cache.revision().await, Ok(Some(revision)) if revision == manifest.revision)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        convert::Infallible,
        env,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use file_system::{MemoryTransport, PeerCodec};
    use idp_model::contract::{GlobalIdentityRow, GlobalIdentityTable, GlobalIdentityValue};

    use crate::GlobalIdentityCache;

    use super::{ActiveGlobalIdentityReadGate, GlobalIdentityReadGate};

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

    fn application() -> GlobalIdentityRow {
        let mut columns = BTreeMap::new();
        columns.insert(
            "name".to_owned(),
            GlobalIdentityValue::Text("app".to_owned()),
        );
        columns.insert(
            "uri".to_owned(),
            GlobalIdentityValue::Text("app".to_owned()),
        );
        columns.insert("description".to_owned(), GlobalIdentityValue::Null);
        columns.insert("created_at".to_owned(), GlobalIdentityValue::Integer(1));
        columns.insert("updated_at".to_owned(), GlobalIdentityValue::Integer(1));
        GlobalIdentityRow {
            table: GlobalIdentityTable::Applications,
            id: 1,
            columns,
        }
    }

    #[tokio::test]
    async fn requires_the_cache_to_match_the_active_revision() {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let root = env::temp_dir().join(format!(
            "global-identity-read-gate-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let (transport, _) = MemoryTransport::pair(Peer, Peer);
        let runtime = Arc::new(
            storage_service::GlobalIdentityRuntime::<Peer, _>::new(root, Peer, transport)
                .await
                .unwrap(),
        );
        let database = Arc::new(
            libsql::Builder::new_local(env::temp_dir().join(format!(
                "global-identity-read-gate-{}-{}.db",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            )))
            .build()
            .await
            .unwrap(),
        );
        idp_model::migrate::up(&database).await.unwrap();
        let cache = GlobalIdentityCache::new(database);
        let row = application();
        let manifest = runtime
            .stage_revision("one".to_owned(), vec![row.clone()])
            .await
            .unwrap();
        runtime.activate_revision(&manifest.revision).await.unwrap();
        let gate = ActiveGlobalIdentityReadGate::new(Arc::clone(&runtime), cache.clone());

        assert!(!gate.verify().await);
        cache.apply(&manifest, &[row]).await.unwrap();
        assert!(gate.verify().await);
    }
}
