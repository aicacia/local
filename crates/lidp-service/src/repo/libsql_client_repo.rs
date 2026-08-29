use std::sync::Arc;

use db::run_transaction;
use libsql::{Database, de::from_row};
use lidp_model::{
    contract::{ClientRegistration, ClientType, EntityType},
    model::Client,
};

use crate::repo::{ClientRepo, KeyService, LibSqlKeyRepo, PrivateKeyRepo, RepoError, RepoResult};

pub struct LibSqlClientRepo {
    database: Arc<Database>,
    key_service: Arc<KeyService<LibSqlKeyRepo>>,
}

impl LibSqlClientRepo {
    pub fn new(database: Arc<Database>, key_service: Arc<KeyService<LibSqlKeyRepo>>) -> Self {
        Self {
            database,
            key_service,
        }
    }
}

impl ClientRepo for LibSqlClientRepo {
    async fn find_client_by_client_id(&self, client_id: &str) -> RepoResult<Option<Client>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT
                id,
                application_id,
                client_id,
                client_secret,
                client_id_issued_at,
                client_secret_expires_at,
                client_name,
                client_uri,
                redirect_uris,
                client_type,
                profile,
                token_endpoint_auth_method,
                allowed_grant_types,
                response_types,
                allowed_scopes,
                logo_uri,
                contacts,
                terms_of_service_uri,
                policy_uri,
                software_statement,
                software_id,
                software_version,
                created_at,
                updated_at
            FROM clients
            WHERE client_id = ?
        "#;
        let row = {
            let mut rows = connection.query(query, libsql::params![client_id]).await?;
            if let Some(row) = rows.next().await? {
                row
            } else {
                return Ok(None);
            }
        };

        let client = from_row::<Client>(&row)?;
        Ok(Some(client))
    }

    async fn list_clients(&self, offset: u32, limit: u32) -> RepoResult<Vec<Client>> {
        let connection = self.database.connect()?;
        let query = r#"
            SELECT
                id,
                application_id,
                client_id,
                client_secret,
                client_id_issued_at,
                client_secret_expires_at,
                client_name,
                client_uri,
                redirect_uris,
                client_type,
                profile,
                token_endpoint_auth_method,
                allowed_grant_types,
                response_types,
                allowed_scopes,
                logo_uri,
                contacts,
                terms_of_service_uri,
                policy_uri,
                software_statement,
                software_id,
                software_version,
                created_at,
                updated_at
            FROM clients
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
        "#;
        let mut rows = connection
            .query(query, libsql::params![i64::from(limit), i64::from(offset)])
            .await?;
        let mut clients = Vec::new();

        while let Some(row) = rows.next().await? {
            clients.push(from_row::<Client>(&row)?);
        }

        Ok(clients)
    }

    async fn create_client(&self, client: ClientRegistration) -> RepoResult<Client> {
        let connection = self.database.connect()?;
        let redirect_uris = serde_json::to_string(&client.redirect_uris)
            .map_err(|e| RepoError::Other(Box::new(e)))?;
        let allowed_grant_types = serde_json::to_string(&client.allowed_grant_types)
            .map_err(|e| RepoError::Other(Box::new(e)))?;
        let response_types = serde_json::to_string(&client.response_types)
            .map_err(|e| RepoError::Other(Box::new(e)))?;
        let allowed_scopes = serde_json::to_string(&client.allowed_scopes)
            .map_err(|e| RepoError::Other(Box::new(e)))?;
        let contacts =
            serde_json::to_string(&client.contacts).map_err(|e| RepoError::Other(Box::new(e)))?;

        let key_service = self.key_service.clone();

        let (_id, client_id) = run_transaction(&connection, move |transaction| {
            Box::pin(async move {
                let client_type = client.client_type;
                let client_secret = client.client_secret.clone();
                let client_name = client.client_name.clone();

                let mut rows = transaction
                    .query(
                        "SELECT id FROM applications WHERE uri = ?",
                        libsql::params![client.application_uri.clone()],
                    )
                    .await?;

                let application_id: i64 = if let Some(row) = rows.next().await? {
                    row.get(0)?
                } else {
                    return Err(RepoError::InvalidInput(format!(
                        "application_uri not found: {:?}",
                        client.application_uri
                    ))
                    .into_libsql());
                };

                let client_query = r#"
                INSERT INTO clients (
                    application_id,
                    client_id,
                    client_secret,
                    client_secret_expires_at,
                    client_name,
                    client_uri,
                    redirect_uris,
                    client_type,
                    profile,
                    token_endpoint_auth_method,
                    allowed_grant_types,
                    response_types,
                    allowed_scopes,
                    logo_uri,
                    contacts,
                    terms_of_service_uri,
                    policy_uri,
                    software_statement,
                    software_id,
                    software_version
                )
                VALUES
                (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                RETURNING id, client_id;"#;
                let mut rows = transaction
                    .query(
                        client_query,
                        libsql::params![
                            application_id,
                            client.client_id,
                            client_secret.clone(),
                            client.client_secret_expires_at,
                            client_name.clone(),
                            client.client_uri,
                            redirect_uris,
                            client_type as i64,
                            client.profile as i64,
                            client.token_endpoint_auth_method as i64,
                            allowed_grant_types,
                            response_types,
                            allowed_scopes,
                            client.logo_uri,
                            contacts,
                            client.terms_of_service_uri,
                            client.policy_uri,
                            client.software_statement,
                            client.software_id,
                            client.software_version,
                        ],
                    )
                    .await?;
                let row = rows.next().await?.ok_or_else(|| libsql::Error::NullValue)?;
                let id: i64 = row.get(0)?;
                let client_id: String = row.get(1)?;

                let passphrase = match client_type {
                    ClientType::Confidential => {
                        let client_secret = client_secret.clone().ok_or_else(|| {
                            RepoError::InvalidInput(
                                "client_secret is required for confidential client key material"
                                    .to_string(),
                            )
                            .into_libsql()
                        })?;

                        if client_secret.trim().is_empty() {
                            return Err(RepoError::InvalidInput(
                                "client_secret is required for confidential client key material"
                                    .to_string(),
                            )
                            .into_libsql());
                        }

                        client_secret
                    }
                    ClientType::Public => String::new(),
                };

                key_service
                    .ensure_entity_master_key(EntityType::Client, id, passphrase.as_str())
                    .map_err(RepoError::into_libsql)?;

                let key = key_service
                    .key_repo()
                    .tx_create_key(
                        None,
                        EntityType::Client,
                        id,
                        true,
                        client_name,
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
                        &key_service.scoped_namespace(EntityType::Client, id),
                        derivation_path,
                    )
                    .map_err(RepoError::into_libsql)?;

                Ok((id, client_id))
            })
        })
        .await?;

        self.find_client_by_client_id(&client_id)
            .await?
            .ok_or_else(|| libsql::Error::Misuse("client not found after insert".into()).into())
    }

    async fn update_client(&self, client: Client) -> RepoResult<Client> {
        let connection = self.database.connect()?;
        let updated_client_id = client.client_id.clone();
        let redirect_uris = serde_json::to_string(&client.redirect_uris)
            .map_err(|e| RepoError::Other(Box::new(e)))?;
        let allowed_grant_types = serde_json::to_string(&client.allowed_grant_types)
            .map_err(|e| RepoError::Other(Box::new(e)))?;
        let response_types = serde_json::to_string(&client.response_types)
            .map_err(|e| RepoError::Other(Box::new(e)))?;
        let allowed_scopes = serde_json::to_string(&client.allowed_scopes)
            .map_err(|e| RepoError::Other(Box::new(e)))?;
        let contacts =
            serde_json::to_string(&client.contacts).map_err(|e| RepoError::Other(Box::new(e)))?;

        connection
            .execute(
                "\
                UPDATE clients SET
                    application_id = ?,
                    client_secret = ?,
                    client_id_issued_at = ?,
                    client_secret_expires_at = ?,
                    client_name = ?,
                    client_uri = ?,
                    redirect_uris = ?,
                    client_type = ?,
                    profile = ?,
                    token_endpoint_auth_method = ?,
                    allowed_grant_types = ?,
                    response_types = ?,
                    allowed_scopes = ?,
                    logo_uri = ?,
                    contacts = ?,
                    terms_of_service_uri = ?,
                    policy_uri = ?,
                    software_statement = ?,
                    software_id = ?,
                    software_version = ?,
                    updated_at = ?
                WHERE client_id = ?",
                libsql::params![
                    client.application_id,
                    client.client_secret,
                    client.client_id_issued_at.map(|d| d.timestamp()),
                    client.client_secret_expires_at.map(|d| d.timestamp()),
                    client.client_name,
                    client.client_uri,
                    redirect_uris,
                    client.client_type as i64,
                    client.profile as i64,
                    client.token_endpoint_auth_method as i64,
                    allowed_grant_types,
                    response_types,
                    allowed_scopes,
                    client.logo_uri,
                    contacts,
                    client.terms_of_service_uri,
                    client.policy_uri,
                    client.software_statement,
                    client.software_id,
                    client.software_version,
                    client.updated_at.timestamp(),
                    client.client_id,
                ],
            )
            .await?;

        self.find_client_by_client_id(&updated_client_id)
            .await?
            .ok_or_else(|| libsql::Error::Misuse("client not found after update".into()).into())
    }

    async fn delete_client_by_client_id(&self, client_id: &str) -> RepoResult<()> {
        let connection = self.database.connect()?;
        connection
            .execute(
                "DELETE FROM clients WHERE client_id = ?",
                libsql::params![client_id],
            )
            .await?;
        Ok(())
    }
}
