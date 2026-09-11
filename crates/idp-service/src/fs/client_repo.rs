use std::{collections::BTreeSet, sync::Arc};

use crate::repo::{ApplicationRepo, ClientRepo, KeyService, PrivateKeyRepo, RepoError, RepoResult};
use chrono::{DateTime, Utc};
use file_system::{FileSystem, PeerCodec, Storage, Transport};
use idp_model::{
    contract::{ClientRegistration, ClientType, EntityType},
    model::{Application, Client},
};

use super::{FsApplicationRepo, FsKeyRepo, JsonStore};

const CLIENTS_FOLDER: &str = "idp/clients";

pub struct FsClientRepo<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>, P> {
    store: JsonStore<S, C, T>,
    application_repo: FsApplicationRepo<S, C, T>,
    key_service: Arc<KeyService<FsKeyRepo<S, C, T>, P>>,
}

impl<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>, P> Clone
    for FsClientRepo<S, C, T, P>
{
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            application_repo: self.application_repo.clone(),
            key_service: Arc::clone(&self.key_service),
        }
    }
}

impl<S, C, T, P> FsClientRepo<S, C, T, P>
where
    S: Storage + Send + Sync + 'static,
    S::Error: std::fmt::Display + Send + 'static,
    C: PeerCodec + Send + 'static,
    C::Error: Send + 'static,
    C::PeerId: Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: 'static,
    P: PrivateKeyRepo,
{
    #[must_use]
    pub fn new(
        file_system: Arc<FileSystem<S, C, T>>,
        key_service: Arc<KeyService<FsKeyRepo<S, C, T>, P>>,
    ) -> Self {
        Self {
            store: JsonStore::new(Arc::clone(&file_system)),
            application_repo: FsApplicationRepo::new(file_system),
            key_service,
        }
    }

    async fn clients(&self) -> RepoResult<Vec<Client>> {
        let paths = self.store.list(CLIENTS_FOLDER).await?;
        let mut clients: Vec<Client> = Vec::with_capacity(paths.len());
        for path in paths {
            clients.push(self.store.read(&path).await?);
        }
        clients.sort_by_key(|client| client.created_at);
        Ok(clients)
    }

    async fn next_id(&self) -> RepoResult<i64> {
        // ponytail: scan records; replace with a replicated ID allocator only if client creation is hot.
        let ids = self
            .clients()
            .await?
            .into_iter()
            .map(|client| client.id)
            .collect::<BTreeSet<_>>();
        random_id(&ids)
    }

    async fn application(&self, registration: &ClientRegistration) -> RepoResult<Application> {
        if let Some(application) = self
            .application_repo
            .find_by_uri(&registration.application.uri)
            .await?
        {
            return Ok(application);
        }
        self.application_repo
            .create_application(
                registration
                    .application
                    .name
                    .clone()
                    .unwrap_or_else(|| registration.client_name.clone()),
                registration.application.uri.clone(),
                registration.application.description.clone(),
            )
            .await
    }
}

impl<S, C, T, P> ClientRepo for FsClientRepo<S, C, T, P>
where
    S: Storage + Send + Sync + 'static,
    S::Error: std::fmt::Display + Send + 'static,
    C: PeerCodec + Send + 'static,
    C::Error: Send + 'static,
    C::PeerId: Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: 'static,
    P: PrivateKeyRepo,
{
    async fn find_client_by_client_id(&self, client_id: &str) -> RepoResult<Option<Client>> {
        Ok(self
            .clients()
            .await?
            .into_iter()
            .find(|client| client.client_id == client_id))
    }

    async fn list_clients(&self, offset: u32, limit: u32) -> RepoResult<Vec<Client>> {
        Ok(self
            .clients()
            .await?
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect())
    }

    async fn create_client(&self, registration: ClientRegistration) -> RepoResult<Client> {
        let client_id = registration
            .client_id
            .clone()
            .ok_or_else(|| RepoError::InvalidInput("client_id is required".into()))?;
        if self.find_client_by_client_id(&client_id).await?.is_some() {
            return Err(RepoError::InvalidInput("client_id already exists".into()));
        }
        let client_secret = match registration.client_type {
            ClientType::Confidential => registration
                .client_secret
                .clone()
                .filter(|secret| !secret.trim().is_empty())
                .ok_or_else(|| {
                    RepoError::InvalidInput(
                        "client_secret is required for confidential client key material".into(),
                    )
                })?,
            ClientType::Public => registration.client_secret.clone().unwrap_or_default(),
        };
        let application = self.application(&registration).await?;
        let now = Utc::now();
        let client = Client {
            id: self.next_id().await?,
            application_id: application.id,
            client_id,
            client_secret: client_secret.clone(),
            client_id_issued_at: registration
                .client_id_issued_at
                .and_then(|timestamp| DateTime::from_timestamp(timestamp, 0)),
            client_secret_expires_at: registration
                .client_secret_expires_at
                .and_then(|timestamp| DateTime::from_timestamp(timestamp, 0)),
            client_name: registration.client_name,
            client_uri: registration
                .client_uri
                .unwrap_or(registration.application.uri),
            redirect_uris: registration.redirect_uris,
            client_type: registration.client_type,
            profile: registration.profile,
            token_endpoint_auth_method: registration.token_endpoint_auth_method,
            allowed_grant_types: registration.allowed_grant_types,
            response_types: registration.response_types,
            allowed_scopes: registration.allowed_scopes,
            logo_uri: registration.logo_uri,
            contacts: registration.contacts,
            terms_of_service_uri: registration.terms_of_service_uri,
            policy_uri: registration.policy_uri,
            software_statement: registration.software_statement,
            software_id: registration.software_id,
            software_version: registration.software_version,
            created_at: now,
            updated_at: now,
        };
        self.store.write(&path(&client.client_id), &client).await?;
        self.key_service
            .ensure_entity_master_key(EntityType::Client, client.id, &client_secret)?;
        self.key_service
            .create_key(
                None,
                EntityType::Client,
                client.id,
                true,
                client.client_name.clone(),
                None,
            )
            .await?;
        Ok(client)
    }

    async fn update_client(&self, mut client: Client) -> RepoResult<Client> {
        if self
            .find_client_by_client_id(&client.client_id)
            .await?
            .is_none()
        {
            return Err(RepoError::InvalidInput("client was not found".into()));
        }
        client.updated_at = Utc::now();
        self.store.write(&path(&client.client_id), &client).await?;
        Ok(client)
    }

    async fn delete_client_by_client_id(&self, client_id: &str) -> RepoResult<()> {
        if self.find_client_by_client_id(client_id).await?.is_none() {
            return Err(RepoError::InvalidInput("client was not found".into()));
        }
        self.store.delete(&path(client_id)).await
    }
}

fn random_id(ids: &BTreeSet<i64>) -> RepoResult<i64> {
    loop {
        let mut bytes = [0_u8; 8];
        getrandom::fill(&mut bytes).map_err(RepoError::other)?;
        let id = i64::from_be_bytes(bytes) & i64::MAX;
        if id != 0 && !ids.contains(&id) {
            return Ok(id);
        }
    }
}

fn path(client_id: &str) -> String {
    let mut name = String::with_capacity(client_id.len() * 2);
    for byte in client_id.bytes() {
        use std::fmt::Write;
        write!(&mut name, "{byte:02x}").expect("writing to String cannot fail");
    }
    format!("{CLIENTS_FOLDER}/{name}.json")
}
