use std::sync::Arc;

use file_system::{PeerCodec, Transport};
use idp_model::contract::{GlobalIdentityManifest, GlobalIdentityRow};
use storage_service::GlobalIdentityRuntime;

use crate::global_identity_cache::{GlobalIdentityCache, validate_snapshot};

pub struct GlobalIdentityRevisionWriter<C, T>
where
    C: PeerCodec,
    T: Transport<PeerId = C::PeerId>,
{
    runtime: Arc<GlobalIdentityRuntime<C, T>>,
    cache: GlobalIdentityCache,
}

impl<C, T> GlobalIdentityRevisionWriter<C, T>
where
    C: PeerCodec + Send + Sync + 'static,
    C::Error: std::fmt::Display + Send + 'static,
    C::PeerId: Send + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Error: std::fmt::Display,
    T::Incoming: Send + 'static,
{
    #[must_use]
    pub fn new(runtime: Arc<GlobalIdentityRuntime<C, T>>, cache: GlobalIdentityCache) -> Self {
        Self { runtime, cache }
    }

    pub async fn apply<F>(
        &self,
        revision: String,
        mutate: F,
    ) -> Result<GlobalIdentityManifest, String>
    where
        F: FnOnce(&mut Vec<GlobalIdentityRow>) -> Result<(), String>,
    {
        let mut rows = self
            .runtime
            .active_rows()
            .await?
            .map_or_else(Vec::new, |(_, rows)| rows);
        mutate(&mut rows)?;
        validate_snapshot(&rows)?;
        let manifest = self.runtime.stage_revision(revision, rows.clone()).await?;
        self.runtime.activate_revision(&manifest.revision).await?;
        self.cache.apply(&manifest, &rows).await?;
        Ok(manifest)
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

    use crate::global_identity_cache::GlobalIdentityCache;

    use super::GlobalIdentityRevisionWriter;

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

    async fn database() -> Arc<libsql::Database> {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        Arc::new(
            libsql::Builder::new_local(env::temp_dir().join(format!(
                "global-identity-writer-{}-{}.db",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            )))
            .build()
            .await
            .unwrap(),
        )
    }

    #[tokio::test]
    async fn stages_activates_and_projects_only_the_mutated_global_snapshot() {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let root = env::temp_dir().join(format!(
            "global-identity-writer-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let (transport, _) = MemoryTransport::pair(Peer, Peer);
        let runtime = Arc::new(
            storage_service::GlobalIdentityRuntime::<Peer, _>::new(root, Peer, transport)
                .await
                .unwrap(),
        );
        let database = database().await;
        idp_model::migrate::up(&database).await.unwrap();
        let cache = GlobalIdentityCache::new(database);
        let writer = GlobalIdentityRevisionWriter::new(Arc::clone(&runtime), cache.clone());

        let manifest = writer
            .apply("one".to_owned(), |rows| {
                rows.push(application());
                Ok(())
            })
            .await
            .unwrap();

        assert_eq!(manifest.revision, "one");
        assert_eq!(cache.revision().await.unwrap().as_deref(), Some("one"));
        assert_eq!(
            runtime.active_rows().await.unwrap().unwrap().1,
            vec![application()]
        );
    }
}
