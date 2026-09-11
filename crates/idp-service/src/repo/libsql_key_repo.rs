use std::sync::Arc;

use chrono::{DateTime, Utc};
use idp_model::{contract::EntityType, model::Key};
use libsql::{Database, de::from_row};

use crate::repo::{KeyRepo, RepoError, RepoResult};

pub struct LibSqlKeyRepo {
    database: Arc<Database>,
}

impl LibSqlKeyRepo {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
}

impl KeyRepo for LibSqlKeyRepo {
    async fn list_active(&self) -> RepoResult<Vec<Key>> {
        let conn = self.database.connect()?;
        let query = r#"
            SELECT
                id,
                parent_id,
                entity_type,
                entity_id,
                derivation_path,
                name,
                hardened,
                revoked_at,
                expires_at,
                created_at,
                updated_at
            FROM keys
            WHERE (revoked_at IS NULL OR revoked_at > unixepoch())
                AND (expires_at IS NULL OR expires_at > unixepoch())
        "#;

        let mut rows = conn.query(query, libsql::params![]).await?;
        let mut keys = Vec::new();

        while let Some(row) = rows.next().await? {
            keys.push(from_row::<Key>(&row)?);
        }

        Ok(keys)
    }

    async fn list_by_entity_type_and_id(
        &self,
        entity_type: EntityType,
        entity_id: i64,
    ) -> RepoResult<Vec<Key>> {
        let conn = self.database.connect()?;
        let query = r#"
            SELECT
                id,
                parent_id,
                entity_type,
                entity_id,
                derivation_path,
                name,
                hardened,
                revoked_at,
                expires_at,
                created_at,
                updated_at
            FROM keys
            WHERE entity_type = ? AND entity_id = ?
                AND (revoked_at IS NULL OR revoked_at > unixepoch())
                AND (expires_at IS NULL OR expires_at > unixepoch())
            ORDER BY created_at DESC
        "#;

        let mut rows = conn
            .query(query, libsql::params![entity_type as i64, entity_id])
            .await?;
        let mut keys = Vec::new();

        while let Some(row) = rows.next().await? {
            keys.push(from_row::<Key>(&row)?);
        }

        Ok(keys)
    }

    async fn find_by_id(&self, id: u32) -> RepoResult<Option<Key>> {
        let conn = self.database.connect()?;
        let query = r#"
            SELECT
                id,
                parent_id,
                entity_type,
                entity_id,
                derivation_path,
                name,
                hardened,
                revoked_at,
                expires_at,
                created_at,
                updated_at
            FROM keys
            WHERE id = ?
                AND (revoked_at IS NULL OR revoked_at > unixepoch())
                AND (expires_at IS NULL OR expires_at > unixepoch())
            LIMIT 1
        "#;

        let mut rows = conn.query(query, libsql::params![id]).await?;
        if let Some(row) = rows.next().await? {
            Ok(Some(from_row::<Key>(&row)?))
        } else {
            Ok(None)
        }
    }

    async fn find_by_entity_type_and_id(
        &self,
        entity_type: EntityType,
        entity_id: i64,
    ) -> RepoResult<Option<Key>> {
        self.find_active_entity_root_key(entity_type, entity_id)
            .await
    }

    async fn create_key(
        &self,
        parent_id: Option<u32>,
        entity_type: EntityType,
        entity_id: i64,
        hardened: bool,
        name: String,
        expires_at: Option<DateTime<Utc>>,
    ) -> RepoResult<Key> {
        let connection = self.database.connect()?;
        let tx = connection.transaction().await?;

        let key = self
            .tx_create_key(
                parent_id,
                entity_type,
                entity_id,
                hardened,
                name,
                expires_at,
                &tx,
            )
            .await?;

        tx.commit().await?;

        Ok(key)
    }

    async fn find_active_entity_root_key(
        &self,
        entity_type: EntityType,
        entity_id: i64,
    ) -> RepoResult<Option<Key>> {
        let conn = self.database.connect()?;
        let query = r#"
            SELECT
                id,
                parent_id,
                entity_type,
                entity_id,
                derivation_path,
                name,
                hardened,
                revoked_at,
                expires_at,
                created_at,
                updated_at
            FROM keys
            WHERE entity_type = ? AND entity_id = ?
                AND (revoked_at IS NULL OR revoked_at > unixepoch())
                AND (expires_at IS NULL OR expires_at > unixepoch())
            ORDER BY created_at DESC
            LIMIT 1
        "#;

        let mut rows = conn
            .query(query, libsql::params![entity_type as i64, entity_id])
            .await?;

        if let Some(row) = rows.next().await? {
            Ok(Some(from_row::<Key>(&row)?))
        } else {
            Ok(None)
        }
    }
}

impl LibSqlKeyRepo {
    pub async fn tx_create_key(
        &self,
        parent_id: Option<u32>,
        entity_type: EntityType,
        entity_id: i64,
        hardened: bool,
        name: String,
        expires_at: Option<DateTime<Utc>>,
        tx: &libsql::Transaction,
    ) -> RepoResult<Key> {
        let query = r#"
            INSERT INTO keys (
                parent_id,
                entity_type,
                entity_id,
                name,
                hardened,
                expires_at
            )
            VALUES (?, ?, ?, ?, ?, ?)
            RETURNING *;
        "#;

        log::debug!(
            "Creating key with entity_type: {:?}, entity_id: {}, name: {}, hardened: {}, expires_at: {:?}",
            entity_type,
            entity_id,
            name,
            hardened,
            expires_at
        );
        let mut rows = tx
            .query(
                query,
                libsql::params![
                    parent_id,
                    entity_type as i64,
                    entity_id,
                    name,
                    hardened,
                    expires_at.map(|dt| dt.timestamp())
                ],
            )
            .await?;
        let row = rows
            .next()
            .await?
            .ok_or_else(|| RepoError::other("failed to create key"))?;
        let mut key = from_row::<Key>(&row)?;

        let parent_derivation_path: Option<String> = if let Some(parent_id) = parent_id {
            let query = r#"SELECT derivation_path, hardened
                FROM keys
                WHERE id = ?
                    AND (revoked_at IS NULL OR revoked_at > unixepoch())
                    AND (expires_at IS NULL OR expires_at > unixepoch())
                LIMIT 1;"#;

            log::debug!("Finding parent key with id: {}", parent_id);
            let mut rows = tx.query(query, libsql::params![parent_id]).await?;
            if let Some(row) = rows.next().await? {
                Some(row.get(0)?)
            } else {
                None
            }
        } else {
            None
        };

        key.derivation_path =
            Key::build_derivation_path(parent_derivation_path.as_deref(), key.id, hardened);

        log::debug!(
            "Updating key with id: {} to have derivation_path: {}",
            key.id,
            key.derivation_path
        );
        tx.execute(
            r#"UPDATE keys SET derivation_path = ? WHERE id = ?;"#,
            libsql::params![key.derivation_path.to_owned(), key.id],
        )
        .await?;

        Ok(key)
    }
}
