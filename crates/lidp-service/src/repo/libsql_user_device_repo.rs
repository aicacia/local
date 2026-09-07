use std::sync::Arc;

use libsql::{Database, de::from_row};
use lidp_model::{contract::TrustedDevice, model::UserDevice};

use crate::repo::{RepoResult, UserDeviceRepo};

pub struct LibSqlUserDeviceRepo {
    database: Arc<Database>,
}

impl LibSqlUserDeviceRepo {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
}

impl UserDeviceRepo for LibSqlUserDeviceRepo {
    async fn create(
        &self,
        user_id: i64,
        name: String,
        public_key: String,
        address: String,
        enrollment_code_hash: Vec<u8>,
        enrollment_expires_at: i64,
    ) -> RepoResult<UserDevice> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    INSERT INTO user_devices (
                        user_id, name, public_key, address, enrollment_code_hash,
                        enrollment_expires_at, state
                    )
                    SELECT ?, ?, ?, ?,
                        CASE WHEN EXISTS(
                            SELECT 1 FROM user_devices WHERE user_id = ?
                        ) THEN ? ELSE NULL END,
                        CASE WHEN EXISTS(
                            SELECT 1 FROM user_devices WHERE user_id = ?
                        ) THEN ? ELSE NULL END,
                        CASE WHEN EXISTS(
                            SELECT 1 FROM user_devices WHERE user_id = ?
                        ) THEN 0 ELSE 1 END
                    RETURNING id, user_id, name, public_key, address, state,
                        created_at, updated_at, revoked_at
                "#,
                libsql::params![
                    user_id,
                    name,
                    public_key,
                    address,
                    user_id,
                    enrollment_code_hash,
                    user_id,
                    enrollment_expires_at,
                    user_id,
                ],
            )
            .await?;
        let row = rows
            .next()
            .await?
            .ok_or(libsql::Error::QueryReturnedNoRows)?;
        Ok(from_row::<UserDevice>(&row)?)
    }

    async fn create_pairing_invitation(
        &self,
        user_id: i64,
        initiating_public_key: String,
        secret_hash: Vec<u8>,
        expires_at: i64,
    ) -> RepoResult<Option<i64>> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    INSERT INTO device_pairing_invitations (
                        user_id, initiating_public_key, secret_hash, expires_at
                    )
                    SELECT ?, ?, ?, ?
                    WHERE EXISTS(
                        SELECT 1 FROM user_devices
                        WHERE user_id = ? AND public_key = ? AND state = 1
                    )
                    RETURNING id
                "#,
                libsql::params![
                    user_id,
                    initiating_public_key.clone(),
                    secret_hash,
                    expires_at,
                    user_id,
                    initiating_public_key,
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
    ) -> RepoResult<Option<(i64, UserDevice)>> {
        let connection = self.database.connect()?;
        let tx = connection.transaction().await?;
        let (invitation_id, user_id) = {
            let mut rows = tx
                .query(
                    r#"
                        UPDATE device_pairing_invitations
                        SET redeemed_at = unixepoch()
                        WHERE secret_hash = ? AND redeemed_at IS NULL AND expires_at > unixepoch()
                        RETURNING id, user_id
                    "#,
                    libsql::params![secret_hash],
                )
                .await?;
            let Some(row) = rows.next().await? else {
                return Ok(None);
            };
            (row.get::<i64>(0)?, row.get::<i64>(1)?)
        };
        let device = {
            let mut rows = tx
                .query(
                    r#"
                        INSERT INTO user_devices (user_id, name, public_key, address, state)
                        VALUES (?, ?, ?, ?, 0)
                        RETURNING id, user_id, name, public_key, address, state,
                            created_at, updated_at, revoked_at
                    "#,
                    libsql::params![user_id, name, public_key, address],
                )
                .await?;
            let row = rows
                .next()
                .await?
                .ok_or(libsql::Error::QueryReturnedNoRows)?;
            from_row::<UserDevice>(&row)?
        };
        tx.execute(
            "UPDATE device_pairing_invitations SET enrollment_device_id = ? WHERE id = ?",
            libsql::params![device.id, invitation_id],
        )
        .await?;
        tx.commit().await?;
        Ok(Some((invitation_id, device)))
    }

    async fn pending_pairing(
        &self,
        user_id: i64,
        device_id: i64,
    ) -> RepoResult<Option<(i64, UserDevice, String)>> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    SELECT i.id, d.id, d.user_id, d.name, d.public_key, d.address, d.state,
                        d.created_at, d.updated_at, d.revoked_at, i.initiating_public_key
                    FROM device_pairing_invitations i
                    JOIN user_devices d ON d.id = i.enrollment_device_id
                    WHERE i.user_id = ? AND d.id = ? AND d.state = 0 AND i.expires_at > unixepoch()
                "#,
                libsql::params![user_id, device_id],
            )
            .await?;
        let Some(row) = rows.next().await? else {
            return Ok(None);
        };
        Ok(Some((
            row.get(0)?,
            from_row::<UserDevice>(&row)?,
            row.get(10)?,
        )))
    }

    async fn approve_pairing(
        &self,
        user_id: i64,
        device_id: i64,
        invitation_id: i64,
    ) -> RepoResult<Option<UserDevice>> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    UPDATE user_devices
                    SET state = 1, updated_at = unixepoch()
                    WHERE id = ? AND user_id = ? AND state = 0
                        AND EXISTS(
                            SELECT 1 FROM device_pairing_invitations
                            WHERE id = ? AND user_id = ? AND enrollment_device_id = ?
                                AND expires_at > unixepoch()
                        )
                    RETURNING id, user_id, name, public_key, address, state,
                        created_at, updated_at, revoked_at
                "#,
                libsql::params![device_id, user_id, invitation_id, user_id, device_id],
            )
            .await?;
        rows.next()
            .await?
            .map(|row| from_row::<UserDevice>(&row))
            .transpose()
            .map_err(Into::into)
    }

    async fn has_any_by_user_id(&self, user_id: i64) -> RepoResult<bool> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                "SELECT EXISTS(SELECT 1 FROM user_devices WHERE user_id = ?)",
                libsql::params![user_id],
            )
            .await?;
        let row = rows
            .next()
            .await?
            .ok_or(libsql::Error::QueryReturnedNoRows)?;
        Ok(row.get::<i64>(0)? != 0)
    }

    async fn list_by_user_id(&self, user_id: i64) -> RepoResult<Vec<UserDevice>> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    SELECT id, user_id, name, public_key, address, state,
                        created_at, updated_at, revoked_at
                    FROM user_devices
                    WHERE user_id = ?
                    ORDER BY id
                "#,
                libsql::params![user_id],
            )
            .await?;
        let mut devices = Vec::new();
        while let Some(row) = rows.next().await? {
            devices.push(from_row::<UserDevice>(&row)?);
        }
        Ok(devices)
    }

    async fn list_approved_by_user_id(&self, user_id: i64) -> RepoResult<Vec<TrustedDevice>> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    SELECT public_key AS publicKey, address
                    FROM user_devices
                    WHERE user_id = ? AND state = 1
                    ORDER BY id
                "#,
                libsql::params![user_id],
            )
            .await?;
        let mut devices = Vec::new();

        while let Some(row) = rows.next().await? {
            devices.push(from_row::<TrustedDevice>(&row)?);
        }

        Ok(devices)
    }

    async fn are_approved_by_user_id(
        &self,
        user_id: i64,
        local_public_key: &str,
        remote_public_key: &str,
    ) -> RepoResult<bool> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    SELECT COUNT(*)
                    FROM user_devices
                    WHERE user_id = ?
                        AND state = 1
                        AND public_key IN (?, ?)
                "#,
                libsql::params![user_id, local_public_key, remote_public_key],
            )
            .await?;
        let count: i64 = rows
            .next()
            .await?
            .ok_or(libsql::Error::QueryReturnedNoRows)?
            .get(0)?;
        Ok(count == 2)
    }

    async fn has_approved_by_user_id(&self, user_id: i64) -> RepoResult<bool> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                "SELECT EXISTS(SELECT 1 FROM user_devices WHERE user_id = ? AND state = 1)",
                libsql::params![user_id],
            )
            .await?;
        let row = rows
            .next()
            .await?
            .ok_or(libsql::Error::QueryReturnedNoRows)?;
        Ok(row.get::<i64>(0)? != 0)
    }

    async fn approve(
        &self,
        user_id: i64,
        device_id: i64,
        enrollment_code_hash: &[u8],
    ) -> RepoResult<Option<UserDevice>> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    UPDATE user_devices
                    SET state = 1,
                        enrollment_code_hash = NULL,
                        enrollment_expires_at = NULL,
                        updated_at = unixepoch()
                    WHERE id = ?
                        AND user_id = ?
                        AND state = 0
                        AND enrollment_code_hash = ?
                        AND enrollment_expires_at > unixepoch()
                    RETURNING id, user_id, name, public_key, address, state,
                        created_at, updated_at, revoked_at
                "#,
                libsql::params![device_id, user_id, enrollment_code_hash],
            )
            .await?;
        rows.next()
            .await?
            .map(|row| from_row::<UserDevice>(&row))
            .transpose()
            .map_err(Into::into)
    }

    async fn rename(
        &self,
        user_id: i64,
        device_id: i64,
        name: String,
    ) -> RepoResult<Option<UserDevice>> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    UPDATE user_devices
                    SET name = ?, updated_at = unixepoch()
                    WHERE id = ? AND user_id = ? AND state != 2
                    RETURNING id, user_id, name, public_key, address, state,
                        created_at, updated_at, revoked_at
                "#,
                libsql::params![name, device_id, user_id],
            )
            .await?;
        rows.next()
            .await?
            .map(|row| from_row::<UserDevice>(&row))
            .transpose()
            .map_err(Into::into)
    }

    async fn revoke(&self, user_id: i64, device_id: i64) -> RepoResult<bool> {
        let connection = self.database.connect()?;
        let changed = connection
            .execute(
                r#"
                    UPDATE user_devices
                    SET state = 2, revoked_at = unixepoch(), updated_at = unixepoch()
                    WHERE id = ?
                        AND user_id = ?
                        AND (
                            state != 1
                            OR EXISTS(
                                SELECT 1
                                FROM user_devices
                                WHERE user_id = ? AND state = 1 AND id != ?
                            )
                        )
                "#,
                libsql::params![device_id, user_id, user_id, device_id],
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
    use lidp_model::contract::UserDeviceState;

    use super::{LibSqlUserDeviceRepo, UserDeviceRepo};

    #[tokio::test]
    async fn approves_and_revokes_a_device() {
        let path = std::env::temp_dir().join(format!(
            "lidp-user-device-test-{}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let database = Arc::new(Builder::new_local(&path).build().await.unwrap());
        lidp_model::migrate::up(&database).await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute("INSERT INTO users (name) VALUES ('user')", ())
            .await
            .unwrap();
        let repo = LibSqlUserDeviceRepo::new(database.clone());
        let first = repo
            .create(
                1,
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
                1,
                "pending".into(),
                "pending-key".into(),
                "pending-address".into(),
                vec![1, 2, 3],
                4_102_444_800,
            )
            .await
            .unwrap();

        assert_eq!(repo.list_approved_by_user_id(1).await.unwrap().len(), 1);
        assert_eq!(
            repo.approve(1, pending.id, &[1, 2, 3])
                .await
                .unwrap()
                .unwrap()
                .state,
            UserDeviceState::Approved
        );
        assert!(repo.revoke(1, pending.id).await.unwrap());
        assert!(!repo.revoke(1, first.id).await.unwrap());
        assert_eq!(repo.list_approved_by_user_id(1).await.unwrap().len(), 1);
        drop(repo);
        drop(database);
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn pairing_invitations_expire_once_and_are_bound_to_the_user() {
        let path =
            std::env::temp_dir().join(format!("lidp-pairing-test-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let database = Arc::new(Builder::new_local(&path).build().await.unwrap());
        lidp_model::migrate::up(&database).await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute("INSERT INTO users (name) VALUES ('first'), ('second')", ())
            .await
            .unwrap();
        let repo = LibSqlUserDeviceRepo::new(database.clone());
        let signer = SecretKey::generate();
        let signer_key = signer.public().to_string();
        repo.create(
            1,
            "first".into(),
            signer_key.clone(),
            "first-address".into(),
            Vec::new(),
            0,
        )
        .await
        .unwrap();

        assert!(
            repo.create_pairing_invitation(1, signer_key.clone(), vec![0], 0)
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
            .create_pairing_invitation(1, signer_key, vec![1], 4_102_444_800)
            .await
            .unwrap()
            .unwrap();
        let (_, pending) = repo
            .redeem_pairing_invitation(&[1], "pending".into(), "key".into(), "address".into())
            .await
            .unwrap()
            .unwrap();
        assert!(
            repo.redeem_pairing_invitation(&[1], "again".into(), "key".into(), "address".into())
                .await
                .unwrap()
                .is_none()
        );
        assert!(repo.pending_pairing(2, pending.id).await.unwrap().is_none());
        assert!(
            repo.approve_pairing(2, pending.id, invitation)
                .await
                .unwrap()
                .is_none()
        );
        drop(repo);
        drop(database);
        std::fs::remove_file(path).unwrap();
    }
}
