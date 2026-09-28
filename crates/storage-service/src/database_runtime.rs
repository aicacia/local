use std::{
    collections::BTreeMap,
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use ofdb::Database;
use storage_model::StorageNamespace;

use crate::{DatabaseCatalog, DatabaseId, DatabaseResource};

pub struct DatabaseRuntime {
    root: PathBuf,
    catalog: DatabaseCatalog,
    open: Mutex<BTreeMap<(String, idp_model::model::Id, DatabaseId), Arc<Database>>>,
}

impl DatabaseRuntime {
    pub fn new(root: PathBuf) -> io::Result<Self> {
        Ok(Self {
            catalog: DatabaseCatalog::new(root.clone())?,
            root,
            open: Mutex::new(BTreeMap::new()),
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
            open.remove(&database_key(scope, id));
        }
        Ok(deleted)
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
    use std::{fs, path::PathBuf};

    use ofdb::SqlTranslator;
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

    #[tokio::test]
    async fn resources_open_independent_durable_engines() {
        let root = PathBuf::from(std::env::temp_dir()).join(format!(
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
