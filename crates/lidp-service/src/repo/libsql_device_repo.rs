use std::sync::Arc;

use libsql::{Database, de::from_row};
use lidp_model::{contract::TrustedDevice, model::Device};

use crate::repo::{DeviceRepo, RepoResult};

pub struct LibSqlDeviceRepo {
    database: Arc<Database>,
}

impl LibSqlDeviceRepo {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
}

impl DeviceRepo for LibSqlDeviceRepo {
    async fn create(
        &self,
        name: String,
        public_key: String,
        address: String,
        enrollment_code_hash: Vec<u8>,
        enrollment_expires_at: i64,
    ) -> RepoResult<Device> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    INSERT INTO devices (
                        name, public_key, address, enrollment_code_hash, enrollment_expires_at,
                        state
                    )
                    SELECT ?, ?, ?,
                        CASE WHEN EXISTS(SELECT 1 FROM devices) THEN ? ELSE NULL END,
                        CASE WHEN EXISTS(SELECT 1 FROM devices) THEN ? ELSE NULL END,
                        CASE WHEN EXISTS(SELECT 1 FROM devices) THEN 0 ELSE 1 END
                    RETURNING id, name, public_key, address, state, created_at, updated_at,
                        revoked_at
                "#,
                libsql::params![
                    name,
                    public_key,
                    address,
                    enrollment_code_hash,
                    enrollment_expires_at,
                ],
            )
            .await?;
        let row = rows
            .next()
            .await?
            .ok_or(libsql::Error::QueryReturnedNoRows)?;
        Ok(from_row::<Device>(&row)?)
    }

    async fn create_pairing(
        &self,
        name: String,
        public_key: String,
        address: String,
        accepting_public_key: String,
    ) -> RepoResult<Device> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    INSERT INTO devices (
                        name, public_key, address, pairing_accepting_public_key, state
                    )
                    VALUES (?, ?, ?, ?, 0)
                    RETURNING id, name, public_key, address, state, created_at, updated_at,
                        revoked_at
                "#,
                libsql::params![name, public_key, address, accepting_public_key],
            )
            .await?;
        let row = rows
            .next()
            .await?
            .ok_or(libsql::Error::QueryReturnedNoRows)?;
        Ok(from_row::<Device>(&row)?)
    }

    async fn pending_pairing(&self, device_id: i64) -> RepoResult<Option<(Device, String)>> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    SELECT id, name, public_key, address, state, created_at, updated_at, revoked_at,
                        pairing_accepting_public_key
                    FROM devices
                    WHERE id = ? AND state = 0 AND pairing_accepting_public_key IS NOT NULL
                "#,
                libsql::params![device_id],
            )
            .await?;
        let Some(row) = rows.next().await? else {
            return Ok(None);
        };
        Ok(Some((from_row::<Device>(&row)?, row.get(8)?)))
    }

    async fn approve_pairing(&self, device_id: i64) -> RepoResult<Option<Device>> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    UPDATE devices
                    SET state = 1, pairing_accepting_public_key = NULL, updated_at = unixepoch()
                    WHERE id = ? AND state = 0 AND pairing_accepting_public_key IS NOT NULL
                    RETURNING id, name, public_key, address, state, created_at, updated_at,
                        revoked_at
                "#,
                libsql::params![device_id],
            )
            .await?;
        rows.next()
            .await?
            .map(|row| from_row::<Device>(&row))
            .transpose()
            .map_err(Into::into)
    }

    async fn has_any(&self) -> RepoResult<bool> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query("SELECT EXISTS(SELECT 1 FROM devices)", ())
            .await?;
        let row = rows
            .next()
            .await?
            .ok_or(libsql::Error::QueryReturnedNoRows)?;
        Ok(row.get::<i64>(0)? != 0)
    }

    async fn list(&self) -> RepoResult<Vec<Device>> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    SELECT id, name, public_key, address, state, created_at, updated_at, revoked_at
                    FROM devices
                    ORDER BY id
                "#,
                (),
            )
            .await?;
        let mut devices = Vec::new();
        while let Some(row) = rows.next().await? {
            devices.push(from_row::<Device>(&row)?);
        }
        Ok(devices)
    }

    async fn list_approved(&self) -> RepoResult<Vec<TrustedDevice>> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    SELECT public_key AS publicKey, address
                    FROM devices
                    WHERE state = 1
                    ORDER BY id
                "#,
                (),
            )
            .await?;
        let mut devices = Vec::new();
        while let Some(row) = rows.next().await? {
            devices.push(from_row::<TrustedDevice>(&row)?);
        }
        Ok(devices)
    }

    async fn are_approved(
        &self,
        local_public_key: &str,
        remote_public_key: &str,
    ) -> RepoResult<bool> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    SELECT COUNT(*)
                    FROM devices
                    WHERE state = 1 AND public_key IN (?, ?)
                "#,
                libsql::params![local_public_key, remote_public_key],
            )
            .await?;
        let count: i64 = rows
            .next()
            .await?
            .ok_or(libsql::Error::QueryReturnedNoRows)?
            .get(0)?;
        Ok(count == 2)
    }

    async fn approve(
        &self,
        device_id: i64,
        enrollment_code_hash: &[u8],
    ) -> RepoResult<Option<Device>> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    UPDATE devices
                    SET state = 1,
                        enrollment_code_hash = NULL,
                        enrollment_expires_at = NULL,
                        updated_at = unixepoch()
                    WHERE id = ?
                        AND state = 0
                        AND enrollment_code_hash = ?
                        AND enrollment_expires_at > unixepoch()
                    RETURNING id, name, public_key, address, state, created_at, updated_at,
                        revoked_at
                "#,
                libsql::params![device_id, enrollment_code_hash],
            )
            .await?;
        rows.next()
            .await?
            .map(|row| from_row::<Device>(&row))
            .transpose()
            .map_err(Into::into)
    }

    async fn rename(&self, device_id: i64, name: String) -> RepoResult<Option<Device>> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    UPDATE devices
                    SET name = ?, updated_at = unixepoch()
                    WHERE id = ? AND state != 2
                    RETURNING id, name, public_key, address, state, created_at, updated_at,
                        revoked_at
                "#,
                libsql::params![name, device_id],
            )
            .await?;
        rows.next()
            .await?
            .map(|row| from_row::<Device>(&row))
            .transpose()
            .map_err(Into::into)
    }

    async fn revoke(&self, device_id: i64, protected_public_key: &str) -> RepoResult<bool> {
        let connection = self.database.connect()?;
        let changed = connection
            .execute(
                r#"
                    UPDATE devices
                    SET state = 2, revoked_at = unixepoch(), updated_at = unixepoch()
                    WHERE id = ?
                        AND public_key != ?
                        AND (
                            state != 1
                            OR EXISTS(
                                SELECT 1
                                FROM devices
                                WHERE state = 1 AND id != ?
                            )
                        )
                "#,
                libsql::params![device_id, protected_public_key, device_id],
            )
            .await?;
        Ok(changed != 0)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use libsql::Builder;
    use lidp_model::contract::DeviceState;

    use super::{DeviceRepo, LibSqlDeviceRepo};

    #[tokio::test]
    async fn manages_global_devices() {
        let path =
            std::env::temp_dir().join(format!("lidp-device-test-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let database = Arc::new(Builder::new_local(&path).build().await.unwrap());
        lidp_model::migrate::up(&database).await.unwrap();
        let repo = LibSqlDeviceRepo::new(Arc::clone(&database));
        let first = repo
            .create(
                "first".into(),
                "first-key".into(),
                "first-address".into(),
                vec![0],
                4_102_444_800,
            )
            .await
            .unwrap();
        let pending = repo
            .create(
                "pending".into(),
                "pending-key".into(),
                "pending-address".into(),
                vec![1, 2, 3],
                4_102_444_800,
            )
            .await
            .unwrap();

        assert_eq!(repo.list_approved().await.unwrap().len(), 1);
        assert_eq!(
            repo.approve(pending.id, &[1, 2, 3])
                .await
                .unwrap()
                .unwrap()
                .state,
            DeviceState::Approved
        );
        assert!(repo.revoke(pending.id, &first.public_key).await.unwrap());
        assert!(!repo.revoke(first.id, &first.public_key).await.unwrap());
        assert_eq!(repo.list_approved().await.unwrap().len(), 1);
        drop(repo);
        drop(database);
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn pairing_request_is_pending_until_approved() {
        let path =
            std::env::temp_dir().join(format!("lidp-pairing-test-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let database = Arc::new(Builder::new_local(&path).build().await.unwrap());
        lidp_model::migrate::up(&database).await.unwrap();
        let repo = LibSqlDeviceRepo::new(Arc::clone(&database));

        let pending = repo
            .create_pairing(
                "pending".into(),
                "key".into(),
                "address".into(),
                "accepting-key".into(),
            )
            .await
            .unwrap();
        let (_, accepting_public_key) = repo.pending_pairing(pending.id).await.unwrap().unwrap();
        assert_eq!(accepting_public_key, "accepting-key");
        assert_eq!(
            repo.approve_pairing(pending.id)
                .await
                .unwrap()
                .unwrap()
                .state,
            DeviceState::Approved
        );
        assert!(repo.pending_pairing(pending.id).await.unwrap().is_none());
        drop(repo);
        drop(database);
        std::fs::remove_file(path).unwrap();
    }
}
