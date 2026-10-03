use std::{
    collections::BTreeMap,
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use ofdb_btree_redb::{Bytes, RedbByteBTree, table_definition};
use ofdb_kv_store::KvStore;
use ofdb_sql::Database;
use redb::Database as RedbDatabase;
use storage_model::StorageNamespace;

use crate::{DatabaseCatalog, DatabaseId, DatabaseResource};

type DatabaseKey = (String, idp_model::model::Id, DatabaseId);
type KvStores = BTreeMap<DatabaseKey, Arc<KvStore<RedbByteBTree>>>;

pub struct DatabaseRuntime {
    root: PathBuf,
    catalog: DatabaseCatalog,
    open: Mutex<BTreeMap<(String, idp_model::model::Id, DatabaseId), Arc<Database>>>,
    kv_open: Mutex<KvStores>,
}

impl DatabaseRuntime {
    pub fn new(root: PathBuf) -> io::Result<Self> {
        Ok(Self {
            catalog: DatabaseCatalog::new(root.clone())?,
            root,
            open: Mutex::new(BTreeMap::new()),
            kv_open: Mutex::new(BTreeMap::new()),
        })
    }

    pub fn create<S: StorageNamespace>(
        &self,
        scope: &S,
        name: Option<String>,
    ) -> io::Result<(DatabaseResource, Arc<Database>)> {
        let mut open = self
            .open
            .lock()
            .map_err(|_| io::Error::other("database runtime lock poisoned"))?;
        let resource = self.catalog.create(scope, name)?;
        let path = database_path(&self.root, scope, resource.id)?;
        match Database::open(&path) {
            Ok(database) => {
                let database = Arc::new(database);
                open.insert(database_key(scope, resource.id), Arc::clone(&database));
                Ok((resource, database))
            }
            Err(error) => {
                self.catalog.delete(scope, resource.id)?;
                Err(io::Error::other(error.to_string()))
            }
        }
    }

    pub fn list<S: StorageNamespace>(&self, scope: &S) -> io::Result<Vec<DatabaseResource>> {
        self.catalog.list(scope)
    }

    pub fn get<S: StorageNamespace>(
        &self,
        scope: &S,
        id: DatabaseId,
    ) -> io::Result<Option<DatabaseResource>> {
        self.catalog.get(scope, id)
    }

    pub fn delete<S: StorageNamespace>(&self, scope: &S, id: DatabaseId) -> io::Result<bool> {
        let mut open = self
            .open
            .lock()
            .map_err(|_| io::Error::other("database runtime lock poisoned"))?;
        let deleted = self.catalog.delete(scope, id)?;
        if deleted {
            self.evict(scope, id, &mut open)?;
        }
        Ok(deleted)
    }

    pub fn apply_tombstone<S: StorageNamespace>(
        &self,
        scope: &S,
        id: DatabaseId,
    ) -> io::Result<()> {
        let mut open = self
            .open
            .lock()
            .map_err(|_| io::Error::other("database runtime lock poisoned"))?;
        self.catalog.apply_tombstone(scope, id)?;
        self.evict(scope, id, &mut open)
    }

    pub fn is_tombstoned<S: StorageNamespace>(
        &self,
        scope: &S,
        id: DatabaseId,
    ) -> io::Result<bool> {
        self.catalog.is_tombstoned(scope, id)
    }

    fn evict<S: StorageNamespace>(
        &self,
        scope: &S,
        id: DatabaseId,
        open: &mut BTreeMap<DatabaseKey, Arc<Database>>,
    ) -> io::Result<()> {
        let key = database_key(scope, id);
        open.remove(&key);
        self.kv_open
            .lock()
            .map_err(|_| io::Error::other("database KV runtime lock poisoned"))?
            .remove(&key);
        Ok(())
    }

    pub fn open_selected<S: StorageNamespace>(
        &self,
        scope: &S,
        id: DatabaseId,
    ) -> io::Result<Option<Arc<Database>>> {
        let mut open = self
            .open
            .lock()
            .map_err(|_| io::Error::other("database runtime lock poisoned"))?;
        if !self.catalog.register_selected(scope, id)? {
            return Ok(None);
        }
        let key = database_key(scope, id);
        if let Some(database) = open.get(&key) {
            return Ok(Some(Arc::clone(database)));
        }
        let path = database_path(&self.root, scope, id)?;
        let database = Arc::new(
            Database::open(path)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?,
        );
        open.insert(key, Arc::clone(&database));
        Ok(Some(database))
    }

    pub fn open_kv_selected<S: StorageNamespace>(
        &self,
        scope: &S,
        id: DatabaseId,
    ) -> io::Result<Option<Arc<KvStore<RedbByteBTree>>>> {
        let mut open = self
            .kv_open
            .lock()
            .map_err(|_| io::Error::other("database KV runtime lock poisoned"))?;
        if !self.catalog.register_selected(scope, id)? {
            return Ok(None);
        }
        let key = database_key(scope, id);
        if let Some(store) = open.get(&key) {
            return Ok(Some(Arc::clone(store)));
        }
        let mut path = database_path(&self.root, scope, id)?;
        path.set_extension("kv.redb");
        let create = !path.exists();
        let database = Arc::new(
            if create {
                RedbDatabase::create(&path)
            } else {
                RedbDatabase::open(&path)
            }
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?,
        );
        if create {
            let transaction = database
                .begin_write()
                .map_err(|error| io::Error::other(error.to_string()))?;
            transaction
                .open_table(table_definition::<Bytes, Vec<u8>>("kv"))
                .map_err(|error| io::Error::other(error.to_string()))?;
            transaction
                .commit()
                .map_err(|error| io::Error::other(error.to_string()))?;
        }
        let store = Arc::new(KvStore::new(RedbByteBTree::new(database, "kv"), || {
            uuid::Timestamp::now(uuid::NoContext)
        }));
        open.insert(key, Arc::clone(&store));
        Ok(Some(store))
    }

    pub fn open<S: StorageNamespace>(
        &self,
        scope: &S,
        id: DatabaseId,
    ) -> io::Result<Option<Arc<Database>>> {
        let mut open = self
            .open
            .lock()
            .map_err(|_| io::Error::other("database runtime lock poisoned"))?;
        if self.catalog.get(scope, id)?.is_none() {
            return Ok(None);
        }
        let key = database_key(scope, id);
        if let Some(database) = open.get(&key) {
            return Ok(Some(Arc::clone(database)));
        }
        let path = database_path(&self.root, scope, id)?;
        let database = Arc::new(
            Database::open(path)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?,
        );
        open.insert(key, Arc::clone(&database));
        Ok(Some(database))
    }
}

fn database_key(
    scope: &impl StorageNamespace,
    id: DatabaseId,
) -> (String, idp_model::model::Id, DatabaseId) {
    (scope.user_sub().to_owned(), scope.application_id(), id)
}

fn database_path(
    root: &std::path::Path,
    scope: &impl StorageNamespace,
    id: DatabaseId,
) -> io::Result<PathBuf> {
    if scope.user_sub().is_empty()
        || matches!(scope.user_sub(), "." | "..")
        || !scope
            .user_sub()
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || scope.application_id().is_nil()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid storage namespace",
        ));
    }
    Ok(root
        .join("databases")
        .join(scope.user_sub())
        .join(scope.application_id().to_string())
        .join(format!("{id}.redb")))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use ofdb_sql::SqlTranslator;
    use storage_model::StorageNamespace;

    use super::DatabaseRuntime;

    struct Scope {
        subject: &'static str,
        application: idp_model::model::Id,
    }

    impl StorageNamespace for Scope {
        fn user_sub(&self) -> &str {
            self.subject
        }

        fn application_id(&self) -> idp_model::model::Id {
            self.application
        }
    }

    #[test]
    fn selected_resource_provisions_matching_id_without_reviving_tombstones() {
        let root = std::env::temp_dir().join(format!(
            "database-runtime-selected-{}-{}",
            std::process::id(),
            idp_model::model::Id::now_v7()
        ));
        let scope = Scope {
            subject: "owner",
            application: idp_model::model::Id::now_v7(),
        };
        let id = idp_model::model::Id::now_v7();
        let runtime = DatabaseRuntime::new(root.clone()).expect("runtime opens");
        assert!(
            runtime
                .open(&scope, id)
                .expect("unprovisioned resource lookup succeeds")
                .is_none()
        );
        let database = runtime
            .open_selected(&scope, id)
            .expect("selected resource opens")
            .expect("selected resource is provisioned");
        assert_eq!(
            runtime
                .list(&scope)
                .expect("catalog lists selected resource")[0]
                .id,
            id
        );
        assert!(runtime.delete(&scope, id).expect("resource tombstones"));
        assert!(
            runtime
                .open_selected(&scope, id)
                .expect("tombstoned resource lookup succeeds")
                .is_none()
        );
        drop(database);
        drop(runtime);
        fs::remove_dir_all(root).expect("test databases are removed");
    }

    #[tokio::test]
    async fn kv_sidecar_persists_across_runtime_reopen_and_deleted_resources_stay_tombstoned() {
        let root = std::env::temp_dir().join(format!(
            "database-runtime-kv-{}-{}",
            std::process::id(),
            idp_model::model::Id::now_v7()
        ));
        let scope = Scope {
            subject: "owner",
            application: idp_model::model::Id::now_v7(),
        };
        let runtime = DatabaseRuntime::new(root.clone()).expect("runtime opens");
        let (resource, _database) = runtime
            .create(&scope, Some("with-kv".into()))
            .expect("database creates");
        let kv_store = runtime
            .open_kv_selected(&scope, resource.id)
            .expect("KV sidecar opens")
            .expect("created resource remains available");
        let mut transaction = kv_store.transaction().await.expect("KV transaction opens");
        transaction
            .set("key", vec![1, 2, 3], None)
            .await
            .expect("KV value writes");
        transaction.commit().await.expect("KV value commits");
        drop(kv_store);
        drop(runtime);

        let runtime = DatabaseRuntime::new(root.clone()).expect("runtime reopens");
        let kv_store = runtime
            .open_kv_selected(&scope, resource.id)
            .expect("persisted KV sidecar opens")
            .expect("resource remains selected");
        let transaction = kv_store
            .transaction()
            .await
            .expect("reopened KV transaction opens");
        assert_eq!(
            transaction.get("key", 0).await.expect("KV value reads"),
            Some(vec![1, 2, 3])
        );
        transaction
            .rollback()
            .await
            .expect("read transaction rolls back");
        assert!(
            runtime
                .delete(&scope, resource.id)
                .expect("resource tombstones")
        );
        drop(kv_store);
        drop(runtime);

        let runtime = DatabaseRuntime::new(root.clone()).expect("runtime reopens after deletion");
        assert!(
            runtime
                .open_kv_selected(&scope, resource.id)
                .expect("tombstoned KV lookup succeeds")
                .is_none()
        );
        drop(runtime);
        fs::remove_dir_all(root).expect("test databases are removed");
    }

    #[tokio::test]
    async fn resources_open_independent_durable_engines() {
        let root = std::env::temp_dir().join(format!(
            "database-runtime-{}-{}",
            std::process::id(),
            idp_model::model::Id::now_v7()
        ));
        let scope = Scope {
            subject: "owner",
            application: idp_model::model::Id::now_v7(),
        };
        let runtime = DatabaseRuntime::new(root.clone()).expect("runtime opens");
        let (first, first_db) = runtime
            .create(&scope, Some("db".into()))
            .expect("first database creates");
        let (second, second_db) = runtime
            .create(&scope, Some("db".into()))
            .expect("second database creates");
        assert_ne!(first.id, second.id);
        first_db
            .translate_and_execute(
                "CREATE TABLE first_table (id UUID PRIMARY KEY)",
                &SqlTranslator,
            )
            .await
            .expect("first schema persists");
        second_db
            .translate_and_execute(
                "CREATE TABLE second_table (id UUID PRIMARY KEY)",
                &SqlTranslator,
            )
            .await
            .expect("second schema persists independently");
        drop(runtime);
        drop(first_db);
        drop(second_db);

        let runtime = DatabaseRuntime::new(root.clone()).expect("runtime reopens");
        let other_subject = Scope {
            subject: "other",
            ..scope
        };
        assert!(
            runtime
                .open(&other_subject, first.id)
                .expect("foreign namespace lookup succeeds")
                .is_none()
        );
        let first_db = runtime
            .open(&scope, first.id)
            .expect("first database opens")
            .expect("first catalog record exists");
        let second_db = runtime
            .open(&scope, second.id)
            .expect("second database opens")
            .expect("second catalog record exists");
        assert_eq!(
            first_db
                .table_schema("first_table")
                .await
                .expect("first schema loads")
                .name,
            "first_table"
        );
        assert!(second_db.table_schema("first_table").await.is_err());
        assert_eq!(
            second_db
                .table_schema("second_table")
                .await
                .expect("second schema loads")
                .name,
            "second_table"
        );
        assert!(
            runtime
                .delete(&scope, first.id)
                .expect("first database deletes")
        );
        assert!(
            runtime
                .open(&scope, first.id)
                .expect("deleted database lookup succeeds")
                .is_none()
        );
        assert_eq!(
            first_db
                .table_schema("first_table")
                .await
                .expect("an already-issued handle remains usable")
                .name,
            "first_table"
        );

        drop(runtime);
        drop(first_db);
        drop(second_db);
        fs::remove_dir_all(root).expect("test databases are removed");
    }
}
