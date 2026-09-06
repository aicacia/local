use std::sync::Arc;

use libsql::{Database, de::from_row};
use lidp_model::contract::TrustedDevice;

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
    async fn list_approved_by_user_id(&self, user_id: i64) -> RepoResult<Vec<TrustedDevice>> {
        let connection = self.database.connect()?;
        let mut rows = connection
            .query(
                r#"
                    SELECT public_key, address
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
}
