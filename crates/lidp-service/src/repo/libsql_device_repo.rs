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

    async fn create_pairing_invitation(
        &self,
        initiating_public_key: String,
        secret_hash: Vec<u8>,
        expires_at: i64,
    ) -> RepoResult<Option<i64>> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    INSERT INTO device_pairing_invitations (
                        initiating_public_key, secret_hash, expires_at
                    )
                    SELECT ?, ?, ?
                    WHERE EXISTS(
                        SELECT 1 FROM devices
                        WHERE public_key = ? AND state = 1
                    )
                    RETURNING id
                "#,
                libsql::params![
                    initiating_public_key.clone(),
                    secret_hash,
                    expires_at,
                    initiating_public_key
                ],
            )
            .await?;
        rows.next()
            .await?
            .map(|row| row.get(0))
            .transpose()
            .map_err(Into::into)
    }

    async fn redeem_pairing_invitation(
        &self,
        secret_hash: &[u8],
        name: String,
        public_key: String,
        address: String,
    ) -> RepoResult<Option<(i64, Device)>> {
        let connection = self.database.connect()?;
        let tx = connection.transaction().await?;
        let invitation_id = {
            let mut rows = tx
                .query(
                    r#"
                        UPDATE device_pairing_invitations
                        SET redeemed_at = unixepoch()
                        WHERE secret_hash = ? AND redeemed_at IS NULL AND expires_at > unixepoch()
                        RETURNING id
                    "#,
                    libsql::params![secret_hash],
                )
                .await?;
            let Some(row) = rows.next().await? else {
                return Ok(None);
            };
            row.get::<i64>(0)?
        };
        let device = {
            let mut rows = tx
                .query(
                    r#"
                        INSERT INTO devices (name, public_key, address, state)
                        VALUES (?, ?, ?, 0)
                        RETURNING id, name, public_key, address, state, created_at, updated_at,
                            revoked_at
                    "#,
                    libsql::params![name, public_key, address],
                )
                .await?;
            let row = rows
                .next()
                .await?
                .ok_or(libsql::Error::QueryReturnedNoRows)?;
            from_row::<Device>(&row)?
        };
        tx.execute(
            "UPDATE device_pairing_invitations SET enrollment_device_id = ? WHERE id = ?",
            libsql::params![device.id, invitation_id],
        )
        .await?;
        tx.commit().await?;
        Ok(Some((invitation_id, device)))
    }

    async fn pending_pairing(&self, device_id: i64) -> RepoResult<Option<(i64, Device, String)>> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    SELECT i.id AS invitation_id, d.id, d.name, d.public_key, d.address, d.state, d.created_at,
                        d.updated_at, d.revoked_at, i.initiating_public_key
                    FROM device_pairing_invitations i
                    JOIN devices d ON d.id = i.enrollment_device_id
                    WHERE d.id = ? AND d.state = 0 AND i.expires_at > unixepoch()
                "#,
                libsql::params![device_id],
            )
            .await?;
        let Some(row) = rows.next().await? else {
            return Ok(None);
        };
        Ok(Some((row.get(0)?, from_row::<Device>(&row)?, row.get(9)?)))
    }

    async fn approve_pairing(
        &self,
        device_id: i64,
        invitation_id: i64,
    ) -> RepoResult<Option<Device>> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    UPDATE devices
                    SET state = 1, updated_at = unixepoch()
                    WHERE id = ? AND state = 0
                        AND EXISTS(
                            SELECT 1 FROM device_pairing_invitations
                            WHERE id = ? AND enrollment_device_id = ? AND expires_at > unixepoch()
                        )
                    RETURNING id, name, public_key, address, state, created_at, updated_at,
                        revoked_at
                "#,
                libsql::params![device_id, invitation_id, device_id],
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

    use iroh::SecretKey;
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
    async fn pairing_invitations_are_global_single_use_and_expiring() {
        let path =
            std::env::temp_dir().join(format!("lidp-pairing-test-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let database = Arc::new(Builder::new_local(&path).build().await.unwrap());
        lidp_model::migrate::up(&database).await.unwrap();
        let repo = LibSqlDeviceRepo::new(Arc::clone(&database));
        let signer = SecretKey::generate();
        let signer_key = signer.public().to_string();
        repo.create(
            "first".into(),
            signer_key.clone(),
            "first-address".into(),
            Vec::new(),
            0,
        )
        .await
        .unwrap();

        assert!(
            repo.create_pairing_invitation(signer_key.clone(), vec![0], 0)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            repo.redeem_pairing_invitation(&[0], "expired".into(), "key".into(), "address".into())
                .await
                .unwrap()
                .is_none()
        );

        let invitation = repo
            .create_pairing_invitation(signer_key, vec![1], 4_102_444_800)
            .await
            .unwrap()
            .unwrap();
        let (_, pending) = repo
            .redeem_pairing_invitation(&[1], "pending".into(), "key".into(), "address".into())
            .await
            .unwrap()
            .unwrap();
        assert!(
            repo.redeem_pairing_invitation(
                &[1],
                "again".into(),
                "other-key".into(),
                "address".into()
            )
            .await
            .unwrap()
            .is_none()
        );
        assert!(repo.pending_pairing(pending.id).await.unwrap().is_some());
        assert!(
            repo.approve_pairing(pending.id, invitation)
                .await
                .unwrap()
                .is_some()
        );
        drop(repo);
        drop(database);
        std::fs::remove_file(path).unwrap();
    }
}
