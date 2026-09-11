use std::sync::Arc;

use chrono::Utc;
use idp_model::model::OAuth2UserConsent;
use libsql::{Database, de::from_row};

use crate::repo::{OAuth2UserConsentRepo, RepoResult};

pub struct LibSqlOAuth2UserConsentRepo {
    database: Arc<Database>,
}

impl LibSqlOAuth2UserConsentRepo {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
}

impl OAuth2UserConsentRepo for LibSqlOAuth2UserConsentRepo {
    async fn upsert_user_consent(
        &self,
        user_id: i64,
        client_id: &str,
        redirect_uri: &str,
        scope: &str,
    ) -> RepoResult<OAuth2UserConsent> {
        let connection = self.database.connect()?;
        let query = r#"
            INSERT INTO oauth2_user_consents (
                user_id,
                client_id,
                redirect_uri,
                scope,
                updated_at
            )
            VALUES (?, ?, ?, ?, ?)
            ON CONFLICT(user_id, client_id, redirect_uri, scope)
            DO UPDATE SET updated_at = excluded.updated_at
            RETURNING
                id,
                user_id,
                client_id,
                redirect_uri,
                scope,
                created_at,
                updated_at
        "#;

        let mut rows = connection
            .query(
                query,
                libsql::params![
                    user_id,
                    client_id,
                    redirect_uri,
                    scope,
                    Utc::now().timestamp(),
                ],
            )
            .await?;

        let row = rows
            .next()
            .await?
            .ok_or_else(|| libsql::Error::Misuse("user consent not found after upsert".into()))?;

        Ok(from_row::<OAuth2UserConsent>(&row)?)
    }

    async fn find_user_consent(
        &self,
        user_id: i64,
        client_id: &str,
        redirect_uri: &str,
        scope: &str,
    ) -> RepoResult<Option<OAuth2UserConsent>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT
                id,
                user_id,
                client_id,
                redirect_uri,
                scope,
                created_at,
                updated_at
            FROM oauth2_user_consents
            WHERE user_id = ? AND client_id = ? AND redirect_uri = ? AND scope = ?
        "#;

        let row = {
            let mut rows = connection
                .query(
                    query,
                    libsql::params![user_id, client_id, redirect_uri, scope],
                )
                .await?;
            if let Some(row) = rows.next().await? {
                row
            } else {
                return Ok(None);
            }
        };

        Ok(Some(from_row::<OAuth2UserConsent>(&row)?))
    }

    async fn list_user_consents(
        &self,
        user_id: i64,
        offset: u32,
        limit: u32,
    ) -> RepoResult<Vec<OAuth2UserConsent>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT
                id,
                user_id,
                client_id,
                redirect_uri,
                scope,
                created_at,
                updated_at
            FROM oauth2_user_consents
            WHERE user_id = ?
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
        "#;

        let mut rows = connection
            .query(
                query,
                libsql::params![user_id, i64::from(limit), i64::from(offset)],
            )
            .await?;

        let mut consents = Vec::new();
        while let Some(row) = rows.next().await? {
            consents.push(from_row::<OAuth2UserConsent>(&row)?);
        }

        Ok(consents)
    }

    async fn find_user_consent_by_id(
        &self,
        consent_id: i64,
    ) -> RepoResult<Option<OAuth2UserConsent>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT
                id,
                user_id,
                client_id,
                redirect_uri,
                scope,
                created_at,
                updated_at
            FROM oauth2_user_consents
            WHERE id = ?
        "#;

        let row = {
            let mut rows = connection.query(query, libsql::params![consent_id]).await?;
            if let Some(row) = rows.next().await? {
                row
            } else {
                return Ok(None);
            }
        };

        Ok(Some(from_row::<OAuth2UserConsent>(&row)?))
    }

    async fn delete_user_consent_by_id(&self, consent_id: i64) -> RepoResult<()> {
        let connection = self.database.connect()?;
        connection
            .execute(
                "DELETE FROM oauth2_user_consents WHERE id = ?",
                libsql::params![consent_id],
            )
            .await?;
        Ok(())
    }
}
