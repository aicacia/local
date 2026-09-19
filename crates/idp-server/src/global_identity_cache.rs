use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use sha2::{Digest, Sha256};

use idp_model::contract::{
    GlobalIdentityManifest, GlobalIdentityRow, GlobalIdentityTable, GlobalIdentityValue,
};
use libsql::Database;

const DELETE_ORDER: [GlobalIdentityTable; 14] = [
    GlobalIdentityTable::ApplicationUserRoles,
    GlobalIdentityTable::RolePermissions,
    GlobalIdentityTable::OAuth2UserConsents,
    GlobalIdentityTable::OAuth2AuthorizationCodes,
    GlobalIdentityTable::UserPasswords,
    GlobalIdentityTable::UserPhoneNumbers,
    GlobalIdentityTable::UserEmails,
    GlobalIdentityTable::Clients,
    GlobalIdentityTable::Keys,
    GlobalIdentityTable::Devices,
    GlobalIdentityTable::Roles,
    GlobalIdentityTable::Permissions,
    GlobalIdentityTable::Users,
    GlobalIdentityTable::Applications,
];

const TABLES: [GlobalIdentityTable; 14] = [
    GlobalIdentityTable::Users,
    GlobalIdentityTable::UserEmails,
    GlobalIdentityTable::UserPhoneNumbers,
    GlobalIdentityTable::UserPasswords,
    GlobalIdentityTable::Applications,
    GlobalIdentityTable::Clients,
    GlobalIdentityTable::Keys,
    GlobalIdentityTable::OAuth2AuthorizationCodes,
    GlobalIdentityTable::OAuth2UserConsents,
    GlobalIdentityTable::Devices,
    GlobalIdentityTable::Roles,
    GlobalIdentityTable::Permissions,
    GlobalIdentityTable::RolePermissions,
    GlobalIdentityTable::ApplicationUserRoles,
];

const INSERT_ORDER: [GlobalIdentityTable; 14] = [
    GlobalIdentityTable::Users,
    GlobalIdentityTable::Applications,
    GlobalIdentityTable::Devices,
    GlobalIdentityTable::Roles,
    GlobalIdentityTable::Permissions,
    GlobalIdentityTable::Keys,
    GlobalIdentityTable::Clients,
    GlobalIdentityTable::UserEmails,
    GlobalIdentityTable::UserPhoneNumbers,
    GlobalIdentityTable::UserPasswords,
    GlobalIdentityTable::OAuth2AuthorizationCodes,
    GlobalIdentityTable::OAuth2UserConsents,
    GlobalIdentityTable::RolePermissions,
    GlobalIdentityTable::ApplicationUserRoles,
];

#[derive(Clone)]
pub struct GlobalIdentityCache {
    database: Arc<Database>,
}

impl GlobalIdentityCache {
    #[must_use]
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }

    pub async fn apply(
        &self,
        manifest: &GlobalIdentityManifest,
        rows: &[GlobalIdentityRow],
    ) -> Result<(), String> {
        if !revision_is_valid(manifest, rows) {
            return Err("invalid global identity revision".to_owned());
        }
        validate_snapshot(rows)?;

        let connection = self.database.connect().map_err(|error| error.to_string())?;
        let transaction = connection
            .transaction()
            .await
            .map_err(|error| error.to_string())?;
        for table in DELETE_ORDER {
            transaction
                .execute(&format!("DELETE FROM {}", table.name()), ())
                .await
                .map_err(|error| error.to_string())?;
        }
        for table in INSERT_ORDER {
            for row in rows.iter().filter(|row| row.table == table) {
                transaction
                    .execute(&insert_statement(row)?, ())
                    .await
                    .map_err(|error| error.to_string())?;
            }
        }
        transaction
            .execute(
                "INSERT INTO global_identity_cache (id, revision) VALUES (1, ?) ON CONFLICT(id) DO UPDATE SET revision = excluded.revision, applied_at = unixepoch()",
                [manifest.revision.as_str()],
            )
            .await
            .map_err(|error| error.to_string())?;
        transaction
            .commit()
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub async fn snapshot(&self) -> Result<Vec<GlobalIdentityRow>, String> {
        let connection = self.database.connect().map_err(|error| error.to_string())?;
        let mut snapshot = Vec::new();
        for table in TABLES {
            let columns = quoted_columns(table.columns());
            let mut rows = connection
                .query(
                    &format!(
                        "SELECT \"id\", {columns} FROM \"{}\" ORDER BY \"id\"",
                        table.name()
                    ),
                    (),
                )
                .await
                .map_err(|error| error.to_string())?;
            while let Some(row) = rows.next().await.map_err(|error| error.to_string())? {
                let id = row.get(0).map_err(|error| error.to_string())?;
                let mut values = BTreeMap::new();
                for (index, column) in table.columns().iter().enumerate() {
                    values.insert(
                        (*column).to_owned(),
                        global_identity_value(
                            row.get_value((index + 1) as i32)
                                .map_err(|error| error.to_string())?,
                        )?,
                    );
                }
                snapshot.push(GlobalIdentityRow {
                    table,
                    id,
                    columns: values,
                });
            }
        }
        validate_snapshot(&snapshot)?;
        Ok(snapshot)
    }

    pub async fn revision(&self) -> Result<Option<String>, String> {
        let connection = self.database.connect().map_err(|error| error.to_string())?;
        let mut rows = connection
            .query(
                "SELECT revision FROM global_identity_cache WHERE id = 1",
                (),
            )
            .await
            .map_err(|error| error.to_string())?;
        let Some(row) = rows.next().await.map_err(|error| error.to_string())? else {
            return Ok(None);
        };
        row.get(0).map(Some).map_err(|error| error.to_string())
    }
}

fn global_identity_value(value: libsql::Value) -> Result<GlobalIdentityValue, String> {
    match value {
        libsql::Value::Null => Ok(GlobalIdentityValue::Null),
        libsql::Value::Integer(value) => Ok(GlobalIdentityValue::Integer(value)),
        libsql::Value::Text(value) => Ok(GlobalIdentityValue::Text(value)),
        libsql::Value::Blob(value) => Ok(GlobalIdentityValue::Blob(value)),
        libsql::Value::Real(_) => {
            Err("real values are not valid global identity values".to_owned())
        }
    }
}

pub(crate) fn validate_snapshot(rows: &[GlobalIdentityRow]) -> Result<(), String> {
    if rows.is_empty() || rows.iter().any(|row| !row.is_valid()) {
        return Err("invalid global identity snapshot".to_owned());
    }
    let ids = |table| {
        rows.iter()
            .filter(|row| row.table == table)
            .map(|row| row.id)
            .collect::<BTreeSet<_>>()
    };
    let users = ids(GlobalIdentityTable::Users);
    let applications = ids(GlobalIdentityTable::Applications);
    let keys = ids(GlobalIdentityTable::Keys);
    let roles = ids(GlobalIdentityTable::Roles);
    let permissions = ids(GlobalIdentityTable::Permissions);
    let clients = rows
        .iter()
        .filter(|row| row.table == GlobalIdentityTable::Clients)
        .filter_map(|row| text(row, "client_id"))
        .collect::<BTreeSet<_>>();
    for row in rows {
        let valid = match row.table {
            GlobalIdentityTable::UserEmails
            | GlobalIdentityTable::UserPhoneNumbers
            | GlobalIdentityTable::UserPasswords => reference(row, "user_id", &users),
            GlobalIdentityTable::Clients
            | GlobalIdentityTable::Roles
            | GlobalIdentityTable::Permissions => reference(row, "application_id", &applications),
            GlobalIdentityTable::Keys => optional_reference(row, "parent_id", &keys),
            GlobalIdentityTable::OAuth2AuthorizationCodes => {
                reference(row, "key_id", &keys)
                    && text(row, "client_id").is_some_and(|client| clients.contains(client))
            }
            GlobalIdentityTable::OAuth2UserConsents => {
                reference(row, "user_id", &users)
                    && text(row, "client_id").is_some_and(|client| clients.contains(client))
            }
            GlobalIdentityTable::RolePermissions => {
                reference(row, "role_id", &roles) && reference(row, "permission_id", &permissions)
            }
            GlobalIdentityTable::ApplicationUserRoles => {
                reference(row, "user_id", &users)
                    && reference(row, "application_id", &applications)
                    && reference(row, "role_id", &roles)
            }
            GlobalIdentityTable::Users
            | GlobalIdentityTable::Applications
            | GlobalIdentityTable::Devices => true,
        };
        if !valid {
            return Err(format!(
                "invalid global identity relationship at {}",
                row.path()
            ));
        }
    }
    Ok(())
}

fn reference(row: &GlobalIdentityRow, column: &str, ids: &BTreeSet<i64>) -> bool {
    matches!(row.columns.get(column), Some(GlobalIdentityValue::Integer(value)) if ids.contains(value))
}

fn optional_reference(row: &GlobalIdentityRow, column: &str, ids: &BTreeSet<i64>) -> bool {
    matches!(row.columns.get(column), Some(GlobalIdentityValue::Null))
        || reference(row, column, ids)
}

fn text<'a>(row: &'a GlobalIdentityRow, column: &str) -> Option<&'a str> {
    match row.columns.get(column) {
        Some(GlobalIdentityValue::Text(value)) => Some(value),
        _ => None,
    }
}

fn revision_is_valid(manifest: &GlobalIdentityManifest, rows: &[GlobalIdentityRow]) -> bool {
    manifest.is_valid()
        && rows.len() == manifest.records.len()
        && rows.iter().zip(&manifest.records).all(|(row, record)| {
            row.is_valid()
                && row.path() == record.path
                && serde_json::to_vec(row)
                    .map(|content| content_hash(&content) == record.hash)
                    .unwrap_or(false)
        })
}

fn content_hash(content: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(content))
}

fn quoted_columns(columns: &[&str]) -> String {
    columns
        .iter()
        .map(|column| format!("\"{column}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

fn insert_statement(row: &GlobalIdentityRow) -> Result<String, String> {
    let columns = row.table.columns();
    let values = columns
        .iter()
        .map(|column| {
            row.columns
                .get(*column)
                .ok_or_else(|| "invalid global identity row".to_owned())
                .map(sql_value)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(format!(
        "INSERT INTO \"{}\" (\"id\", {}) VALUES ({}, {})",
        row.table.name(),
        quoted_columns(columns),
        row.id,
        values.join(", ")
    ))
}

fn sql_value(value: &GlobalIdentityValue) -> String {
    match value {
        GlobalIdentityValue::Null => "NULL".to_owned(),
        GlobalIdentityValue::Integer(value) => value.to_string(),
        GlobalIdentityValue::Text(value) => format!("'{}'", value.replace('\'', "''")),
        GlobalIdentityValue::Blob(value) => format!("X'{}'", hex(value)),
    }
}

fn hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        env,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use file_system::ContentHash;
    use idp_model::contract::{
        GLOBAL_IDENTITY_MANIFEST_VERSION, GlobalIdentityManifest, GlobalIdentityRecord,
        GlobalIdentityRow, GlobalIdentityTable, GlobalIdentityValue,
    };

    use super::GlobalIdentityCache;

    fn application(id: i64, name: &str) -> GlobalIdentityRow {
        let mut columns = BTreeMap::new();
        columns.insert(
            "name".to_owned(),
            GlobalIdentityValue::Text(name.to_owned()),
        );
        columns.insert(
            "uri".to_owned(),
            GlobalIdentityValue::Text(format!("https://{name}")),
        );
        columns.insert("description".to_owned(), GlobalIdentityValue::Null);
        columns.insert("created_at".to_owned(), GlobalIdentityValue::Integer(1));
        columns.insert("updated_at".to_owned(), GlobalIdentityValue::Integer(1));
        GlobalIdentityRow {
            table: GlobalIdentityTable::Applications,
            id,
            columns,
        }
    }

    fn manifest(revision: &str, rows: &[GlobalIdentityRow]) -> GlobalIdentityManifest {
        GlobalIdentityManifest {
            version: GLOBAL_IDENTITY_MANIFEST_VERSION,
            revision: revision.to_owned(),
            records: rows
                .iter()
                .map(|row| GlobalIdentityRecord {
                    path: row.path(),
                    hash: ContentHash::of(&serde_json::to_vec(row).unwrap()).to_string(),
                })
                .collect(),
        }
    }

    async fn database() -> Arc<libsql::Database> {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        Arc::new(
            libsql::Builder::new_local(env::temp_dir().join(format!(
                "global-identity-cache-{}-{}.db",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            )))
            .build()
            .await
            .unwrap(),
        )
    }

    #[tokio::test]
    async fn exports_rows_in_path_order_and_preserves_blobs() {
        let database = database().await;
        idp_model::migrate::up(&database).await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO devices (id, name, public_key, address, enrollment_code_hash, state) VALUES (2, 'later', 'key-2', 'address-2', X'0001', 1), (1, 'first', 'key-1', 'address-1', NULL, 1)",
                (),
            )
            .await
            .unwrap();

        let rows = GlobalIdentityCache::new(database).snapshot().await.unwrap();

        assert_eq!(
            rows.iter().map(GlobalIdentityRow::path).collect::<Vec<_>>(),
            ["records/devices/1.json", "records/devices/2.json",]
        );
        assert_eq!(
            rows[1].columns.get("enrollment_code_hash"),
            Some(&GlobalIdentityValue::Blob(vec![0, 1]))
        );
    }

    #[tokio::test]
    async fn rejects_invalid_cross_table_relationships() {
        let database = database().await;
        idp_model::migrate::up(&database).await.unwrap();
        database
            .connect()
            .unwrap()
            .execute(
                "INSERT INTO roles (id, application_id, name) VALUES (1, 99, 'missing-app')",
                (),
            )
            .await
            .unwrap();

        assert!(GlobalIdentityCache::new(database).snapshot().await.is_err());
    }

    #[tokio::test]
    async fn replaces_the_cache_and_marks_the_same_revision_atomically() {
        let database = database().await;
        idp_model::migrate::up(&database).await.unwrap();
        let cache = GlobalIdentityCache::new(Arc::clone(&database));
        let first = application(1, "first");
        cache
            .apply(
                &manifest("one", std::slice::from_ref(&first)),
                std::slice::from_ref(&first),
            )
            .await
            .unwrap();
        let second = application(2, "second");
        cache
            .apply(
                &manifest("two", std::slice::from_ref(&second)),
                std::slice::from_ref(&second),
            )
            .await
            .unwrap();

        assert_eq!(cache.revision().await.unwrap().as_deref(), Some("two"));
        let connection = database.connect().unwrap();
        let mut rows = connection
            .query("SELECT id, name FROM applications", ())
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        assert_eq!(row.get::<i64>(0).unwrap(), 2);
        assert_eq!(row.get::<String>(1).unwrap(), "second");
        assert!(rows.next().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn rejects_a_manifest_valid_revision_with_invalid_relationships() {
        let database = database().await;
        idp_model::migrate::up(&database).await.unwrap();
        let cache = GlobalIdentityCache::new(database);
        let mut columns = BTreeMap::new();
        columns.insert(
            "application_id".to_owned(),
            GlobalIdentityValue::Integer(99),
        );
        columns.insert(
            "name".to_owned(),
            GlobalIdentityValue::Text("role".to_owned()),
        );
        columns.insert("description".to_owned(), GlobalIdentityValue::Null);
        columns.insert("created_at".to_owned(), GlobalIdentityValue::Integer(1));
        columns.insert("updated_at".to_owned(), GlobalIdentityValue::Integer(1));
        let row = GlobalIdentityRow {
            table: GlobalIdentityTable::Roles,
            id: 1,
            columns,
        };

        assert!(
            cache
                .apply(
                    &manifest("invalid", std::slice::from_ref(&row)),
                    std::slice::from_ref(&row),
                )
                .await
                .is_err()
        );
        assert_eq!(cache.revision().await.unwrap(), None);
    }

    #[tokio::test]
    async fn leaves_the_previous_cache_when_projection_fails() {
        let database = database().await;
        idp_model::migrate::up(&database).await.unwrap();
        let cache = GlobalIdentityCache::new(Arc::clone(&database));
        let row = application(1, "first");
        cache
            .apply(
                &manifest("one", std::slice::from_ref(&row)),
                std::slice::from_ref(&row),
            )
            .await
            .unwrap();
        let invalid = application(2, "first");
        let duplicate = application(3, "first");

        assert!(
            cache
                .apply(
                    &manifest("two", &[invalid.clone(), duplicate.clone()]),
                    &[invalid, duplicate],
                )
                .await
                .is_err()
        );
        assert_eq!(cache.revision().await.unwrap().as_deref(), Some("one"));
    }
}
