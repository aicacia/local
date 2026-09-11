use std::sync::Arc;

use db::run_transaction;
use idp_model::{
    contract::EntityType,
    model::{User, UserEmail, UserPassword, UserPhoneNumber},
};
use libsql::{Database, de::from_row};

use crate::{
    PasswordConfig,
    repo::{KeyService, LibSqlKeyRepo, PrivateKeyRepo, RepoError, RepoResult, UserRepo},
    util::encrypt_password,
};

pub struct LibSqlUserRepo {
    database: Arc<Database>,
    key_service: Arc<KeyService<LibSqlKeyRepo>>,
    password_config: PasswordConfig,
}

impl LibSqlUserRepo {
    pub fn new(
        database: Arc<Database>,
        key_service: Arc<KeyService<LibSqlKeyRepo>>,
        password_config: PasswordConfig,
    ) -> Self {
        Self {
            database,
            key_service,
            password_config,
        }
    }
}

impl UserRepo for LibSqlUserRepo {
    async fn find_user_by_id(&self, user_id: i64) -> RepoResult<Option<User>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT
                id,
                name,
                given_name,
                family_name,
                middle_name,
                nickname,
                profile,
                picture,
                website,
                sex,
                birthdate,
                zoneinfo,
                locale,
                created_at,
                updated_at
            FROM users
            WHERE id = ?
        "#;
        let row = {
            let mut rows = connection.query(query, libsql::params![user_id]).await?;
            if let Some(row) = rows.next().await? {
                row
            } else {
                return Ok(None);
            }
        };
        let user = from_row::<User>(&row)?;
        Ok(Some(user))
    }

    async fn list_users(&self, offset: u32, limit: u32) -> RepoResult<Vec<User>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT
                id,
                name,
                given_name,
                family_name,
                middle_name,
                nickname,
                profile,
                picture,
                website,
                sex,
                birthdate,
                zoneinfo,
                locale,
                created_at,
                updated_at
            FROM users
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
        "#;
        let mut rows = connection
            .query(query, libsql::params![i64::from(limit), i64::from(offset)])
            .await?;
        let mut users = Vec::new();

        while let Some(row) = rows.next().await? {
            users.push(from_row::<User>(&row)?);
        }

        Ok(users)
    }

    async fn find_user_by_username_or_email(&self, identifier: &str) -> RepoResult<Option<User>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT
                u.id,
                u.name,
                u.given_name,
                u.family_name,
                u.middle_name,
                u.nickname,
                u.profile,
                u.picture,
                u.website,
                u.sex,
                u.birthdate,
                u.zoneinfo,
                u.locale,
                u.created_at,
                u.updated_at
            FROM users u
            JOIN user_emails ue ON ue.user_id = u.id
            WHERE lower(ue.email) = lower(?) OR lower(u.name) = lower(?)
            ORDER BY ue.`primary` DESC, ue.id ASC
            LIMIT 1
        "#;
        let row = {
            let mut rows = connection
                .query(query, libsql::params![identifier, identifier])
                .await?;
            if let Some(row) = rows.next().await? {
                row
            } else {
                return Ok(None);
            }
        };

        let user = from_row::<User>(&row)?;
        Ok(Some(user))
    }

    async fn find_user_emails_by_user_id(&self, user_id: i64) -> RepoResult<Vec<UserEmail>> {
        let connection = self.database.connect()?;

        let query =
            r#"SELECT * FROM user_emails where user_id = ? ORDER BY `primary` DESC, id ASC;"#;
        let mut rows = connection.query(query, libsql::params![user_id]).await?;

        let emails = {
            let mut emails = Vec::new();
            while let Some(row) = rows.next().await? {
                let email: UserEmail = from_row(&row)?;
                emails.push(email);
            }
            emails
        };

        Ok(emails)
    }

    async fn find_user_phone_numbers_by_user_id(
        &self,
        user_id: i64,
    ) -> RepoResult<Vec<UserPhoneNumber>> {
        let connection = self.database.connect()?;

        let query = r#"SELECT * FROM user_phone_numbers where user_id = ? ORDER BY `primary` DESC, id ASC;"#;
        let mut rows = connection.query(query, libsql::params![user_id]).await?;

        let phone_numbers = {
            let mut phone_numbers = Vec::new();
            while let Some(row) = rows.next().await? {
                let phone_number: UserPhoneNumber = from_row(&row)?;
                phone_numbers.push(phone_number);
            }
            phone_numbers
        };

        Ok(phone_numbers)
    }

    async fn find_user_password_by_user_id(
        &self,
        user_id: i64,
    ) -> RepoResult<Option<UserPassword>> {
        let connection = self.database.connect()?;

        let query = r#"SELECT * FROM user_passwords where user_id = ? AND active = 1 LIMIT 1;"#;
        let mut rows = connection.query(query, libsql::params![user_id]).await?;

        if let Some(row) = rows.next().await? {
            let password: UserPassword = from_row(&row)?;
            Ok(Some(password))
        } else {
            Ok(None)
        }
    }

    async fn create_user_with_email_and_password(
        &self,
        username: &str,
        email: &str,
        password: &str,
    ) -> RepoResult<User> {
        if password.trim().is_empty() {
            return Err(RepoError::InvalidInput(
                "password is required for user key material".to_string(),
            ));
        }

        let connection = self.database.connect()?;

        let key_service = self.key_service.clone();

        let username = username.to_string();
        let email = email.to_string();
        let password = password.to_string();
        let password_hash = encrypt_password(&self.password_config, password.as_str())
            .map_err(|e| RepoError::Other(e.into()))?;

        let (user, _email, _password) = run_transaction(&connection, move |transaction| {
            Box::pin(async move {
                let user_query = r#"
                INSERT INTO users (name) VALUES (?)
                RETURNING *;"#;

                let mut rows = transaction
                    .query(user_query, libsql::params![username.clone()])
                    .await?;

                let row = rows
                    .next()
                    .await?
                    .ok_or_else(|| libsql::Error::Misuse("user not found after insert".into()))?;
                let user: User = from_row(&row).map_err(|e| {
                    libsql::Error::Misuse(format!("invalid rows returned for user: {}", e))
                })?;

                key_service
                    .ensure_entity_master_key(EntityType::User, user.id, password.as_str())
                    .map_err(RepoError::into_libsql)?;

                let email_query = r#"
                INSERT INTO `user_emails` (
                    `user_id`,
                    `email`,
                    `verified`,
                    `primary`
                ) VALUES (?, ?, ?, ?)
                RETURNING *;"#;

                let mut rows = transaction
                    .query(email_query, libsql::params![user.id, email, false, true])
                    .await?;
                let row = rows.next().await?.ok_or_else(|| {
                    libsql::Error::Misuse("user email not found after insert".into())
                })?;
                let email: UserEmail = from_row(&row).map_err(|e| {
                    libsql::Error::Misuse(format!("invalid rows returned for user email: {}", e))
                })?;

                let password_query = r#"
                INSERT INTO `user_passwords` (
                    `user_id`,
                    `password_hash`
                ) VALUES (?, ?)
                RETURNING *;"#;
                let mut rows = transaction
                    .query(password_query, libsql::params![user.id, password_hash])
                    .await?;
                let row = rows.next().await?.ok_or_else(|| {
                    libsql::Error::Misuse("user password not found after insert".into())
                })?;
                let password: UserPassword = from_row(&row).map_err(|e| {
                    libsql::Error::Misuse(format!("invalid rows returned for user password: {}", e))
                })?;

                let key = key_service
                    .key_repo()
                    .tx_create_key(
                        None,
                        EntityType::User,
                        user.id,
                        true,
                        username,
                        None,
                        transaction,
                    )
                    .await
                    .map_err(RepoError::into_libsql)?;

                let derivation_path = key
                    .derivation_path()
                    .map_err(RepoError::from)
                    .map_err(RepoError::into_libsql)?;

                key_service
                    .private_key_repo()
                    .ensure_derivation_path(
                        &key_service.scoped_namespace(EntityType::User, user.id),
                        derivation_path,
                    )
                    .map_err(RepoError::into_libsql)?;

                Ok((user, email, password))
            })
        })
        .await?;

        Ok(user)
    }

    async fn update_user(&self, user: User) -> RepoResult<User> {
        let connection = self.database.connect()?;
        let sex = user.sex.map(|value| value as i64);
        let birthdate = user.birthdate.map(|value| value.to_rfc3339());

        let query = r#"
            UPDATE users
            SET
                name = ?,
                given_name = ?,
                family_name = ?,
                middle_name = ?,
                nickname = ?,
                profile = ?,
                picture = ?,
                website = ?,
                sex = ?,
                birthdate = ?,
                zoneinfo = ?,
                locale = ?,
                updated_at = unixepoch()
            WHERE id = ?
            RETURNING *
        "#;

        let mut rows = connection
            .query(
                query,
                libsql::params![
                    user.name,
                    user.given_name,
                    user.family_name,
                    user.middle_name,
                    user.nickname,
                    user.profile,
                    user.picture,
                    user.website,
                    sex,
                    birthdate,
                    user.zoneinfo,
                    user.locale,
                    user.id,
                ],
            )
            .await?;

        if let Some(row) = rows.next().await? {
            Ok(from_row::<User>(&row)?)
        } else {
            Err(RepoError::Other("user not found during update".into()))
        }
    }

    async fn upsert_primary_user_email(
        &self,
        user_id: i64,
        email: &str,
        verified: bool,
    ) -> RepoResult<()> {
        let connection = self.database.connect()?;
        let email = email.to_string();

        run_transaction(&connection, move |transaction| {
            Box::pin(async move {
                transaction
                    .execute(
                        "UPDATE user_emails SET `primary` = 0, updated_at = unixepoch() WHERE user_id = ?",
                        libsql::params![user_id],
                    )
                    .await?;

                transaction
                    .execute(
                        r#"
                            INSERT INTO user_emails (user_id, email, verified, `primary`)
                            VALUES (?, ?, ?, 1)
                            ON CONFLICT(user_id, email)
                            DO UPDATE SET
                                verified = excluded.verified,
                                `primary` = 1,
                                updated_at = unixepoch()
                        "#,
                        libsql::params![user_id, email, verified],
                    )
                    .await?;

                Ok(())
            })
        })
        .await?;

        Ok(())
    }

    async fn upsert_primary_user_phone_number(
        &self,
        user_id: i64,
        phone_number: &str,
        verified: bool,
    ) -> RepoResult<()> {
        let connection = self.database.connect()?;
        let phone_number = phone_number.to_string();

        run_transaction(&connection, move |transaction| {
            Box::pin(async move {
                transaction
                    .execute(
                        "UPDATE user_phone_numbers SET `primary` = 0, updated_at = unixepoch() WHERE user_id = ?",
                        libsql::params![user_id],
                    )
                    .await?;

                transaction
                    .execute(
                        r#"
                            INSERT INTO user_phone_numbers (user_id, phone_number, verified, `primary`)
                            VALUES (?, ?, ?, 1)
                            ON CONFLICT(user_id, phone_number)
                            DO UPDATE SET
                                verified = excluded.verified,
                                `primary` = 1,
                                updated_at = unixepoch()
                        "#,
                        libsql::params![user_id, phone_number, verified],
                    )
                    .await?;

                Ok(())
            })
        })
        .await?;

        Ok(())
    }

    async fn replace_user_password(&self, user_id: i64, password: &str) -> RepoResult<()> {
        let connection = self.database.connect()?;

        let password_hash = encrypt_password(&self.password_config, password)
            .map_err(|e| RepoError::Other(e.into()))?;

        run_transaction(&connection, move |transaction| {
            Box::pin(async move {
                transaction
                    .execute(
                        "UPDATE user_passwords SET active = 0, updated_at = unixepoch() WHERE user_id = ? AND active = 1",
                        libsql::params![user_id],
                    )
                    .await?;

                transaction
                    .execute(
                        "INSERT INTO user_passwords (user_id, active, password_hash) VALUES (?, 1, ?)",
                        libsql::params![user_id, password_hash],
                    )
                    .await?;

                Ok(())
            })
        })
        .await?;

        Ok(())
    }

    async fn delete_user_by_id(&self, user_id: i64) -> RepoResult<()> {
        let connection = self.database.connect()?;
        connection
            .execute("DELETE FROM users WHERE id = ?", libsql::params![user_id])
            .await?;
        Ok(())
    }
}
