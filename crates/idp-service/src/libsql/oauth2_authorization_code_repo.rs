use std::sync::Arc;

use chrono::{DateTime, Utc};
use idp_model::{contract::CodeChallengeMethod, model::OAuth2AuthorizationCode};
use libsql::{Database, de::from_row};

use idp_service::{
    generate_random_string,
    repo::{OAuth2AuthorizationCodeRepo, RepoError, RepoResult},
};

pub struct LibSqlOAuth2AuthorizationCodeRepo {
    database: Arc<Database>,
}

impl LibSqlOAuth2AuthorizationCodeRepo {
    pub fn new(database: Arc<Database>) -> Self {
        Self { database }
    }
}

impl OAuth2AuthorizationCodeRepo for LibSqlOAuth2AuthorizationCodeRepo {
    async fn create_authorization_code(
        &self,
        client_id: String,
        key_id: u32,
        redirect_uri: String,
        scopes: Vec<String>,
        resource: Option<String>,
        code_challenge: Option<String>,
        code_challenge_method: Option<CodeChallengeMethod>,
        nonce: Option<String>,
        expires_at: DateTime<Utc>,
    ) -> RepoResult<OAuth2AuthorizationCode> {
        let connection = self.database.connect()?;
        let scopes = serde_json::to_string(&scopes).map_err(|e| RepoError::Other(Box::new(e)))?;

        let query = "\
                INSERT INTO oauth2_authorization_codes (
                    code,
                    client_id,
                    key_id,
                    redirect_uri,
                    scopes,
                    resource,
                    code_challenge,
                    code_challenge_method,
                    nonce,
                    expires_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                RETURNING *;";
        let params = libsql::params![
            generate_random_string::<32>(),
            client_id,
            key_id,
            redirect_uri,
            scopes,
            resource,
            code_challenge,
            code_challenge_method.map(|ccm| ccm as i64),
            nonce,
            expires_at.timestamp(),
        ];
        let mut rows = connection.query(query, params).await?;

        let row = rows.next().await?.ok_or_else(|| {
            libsql::Error::Misuse("authorization code not found after insert".into())
        })?;

        Ok(from_row::<OAuth2AuthorizationCode>(&row)?)
    }

    async fn find_authorization_code_by_code(
        &self,
        code: &str,
    ) -> RepoResult<Option<OAuth2AuthorizationCode>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT
                id,
                code,
                client_id,
                key_id,
                redirect_uri,
                scopes,
                resource,
                code_challenge,
                code_challenge_method,
                nonce,
                expires_at,
                consumed_at,
                created_at,
                updated_at
            FROM oauth2_authorization_codes
            WHERE code = ?
        "#;
        let row = {
            let mut rows = connection.query(query, libsql::params![code]).await?;
            if let Some(row) = rows.next().await? {
                row
            } else {
                return Ok(None);
            }
        };
        Ok(Some(from_row::<OAuth2AuthorizationCode>(&row)?))
    }

    async fn consume_authorization_code(
        &self,
        id: i64,
        consumed_at: DateTime<Utc>,
    ) -> RepoResult<()> {
        let connection = self.database.connect()?;
        connection
            .execute(
                "UPDATE oauth2_authorization_codes \
                 SET consumed_at = ?, updated_at = ? \
                 WHERE id = ? AND consumed_at IS NULL",
                libsql::params![consumed_at.timestamp(), consumed_at.timestamp(), id,],
            )
            .await?;
        Ok(())
    }
}
