use std::sync::Arc;

use crate::repo::{ApplicationRepo, RepoError, RepoResult};
use chrono::Utc;
use file_system::{FileSystem, PeerCodec, Storage, Transport};
use idp_model::model::Application;

use super::JsonStore;

const APPLICATIONS_FOLDER: &str = "idp/applications";

pub struct FsApplicationRepo<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>> {
    store: JsonStore<S, C, T>,
}

impl<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>> Clone
    for FsApplicationRepo<S, C, T>
{
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
        }
    }
}

impl<S, C, T> FsApplicationRepo<S, C, T>
where
    S: Storage + Send + Sync + 'static,
    S::Error: std::fmt::Display + Send + 'static,
    C: PeerCodec + Send + 'static,
    C::Error: Send + 'static,
    C::PeerId: Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: 'static,
{
    #[must_use]
    pub fn new(file_system: Arc<FileSystem<S, C, T>>) -> Self {
        Self {
            store: JsonStore::new(file_system),
        }
    }

    async fn applications(&self) -> RepoResult<Vec<Application>> {
        let paths = self.store.list(APPLICATIONS_FOLDER).await?;
        let mut applications: Vec<Application> = Vec::with_capacity(paths.len());
        for path in paths {
            applications.push(self.store.read(&path).await?);
        }
        applications.sort_by_key(|application| application.id);
        Ok(applications)
    }

    async fn next_id(&self) -> RepoResult<i64> {
        // ponytail: scan records; replace with a replicated ID allocator only if application creation is hot.
        let ids = self
            .applications()
            .await?
            .into_iter()
            .map(|application| application.id)
            .collect::<std::collections::BTreeSet<_>>();
        loop {
            let mut bytes = [0_u8; 8];
            getrandom::fill(&mut bytes).map_err(RepoError::other)?;
            let id = i64::from_be_bytes(bytes) & i64::MAX;
            if id != 0 && !ids.contains(&id) {
                return Ok(id);
            }
        }
    }

    async fn uri_available(&self, uri: &str, id: Option<i64>) -> RepoResult<bool> {
        Ok(self
            .applications()
            .await?
            .into_iter()
            .all(|application| application.uri != uri || Some(application.id) == id))
    }
}

impl<S, C, T> ApplicationRepo for FsApplicationRepo<S, C, T>
where
    S: Storage + Send + Sync + 'static,
    S::Error: std::fmt::Display + Send + 'static,
    C: PeerCodec + Send + 'static,
    C::Error: Send + 'static,
    C::PeerId: Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: 'static,
{
    async fn find_by_id(&self, application_id: i64) -> RepoResult<Option<Application>> {
        match self.store.read(&path(application_id)).await {
            Ok(application) => Ok(Some(application)),
            Err(RepoError::InvalidInput(message)) if message.contains("not found") => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn find_by_uri(&self, uri: &str) -> RepoResult<Option<Application>> {
        Ok(self
            .applications()
            .await?
            .into_iter()
            .find(|application| application.uri == uri))
    }

    async fn list_applications(&self, offset: u32, limit: u32) -> RepoResult<Vec<Application>> {
        Ok(self
            .applications()
            .await?
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect())
    }

    async fn create_application(
        &self,
        name: String,
        uri: String,
        description: Option<String>,
    ) -> RepoResult<Application> {
        if !self.uri_available(&uri, None).await? {
            return Err(RepoError::InvalidInput(
                "application URI already exists".into(),
            ));
        }
        let now = Utc::now();
        let application = Application {
            id: self.next_id().await?,
            name,
            uri,
            description,
            created_at: now,
            updated_at: now,
        };
        self.store
            .write(&path(application.id), &application)
            .await?;
        Ok(application)
    }

    async fn update_application(&self, mut application: Application) -> RepoResult<Application> {
        if self.find_by_id(application.id).await?.is_none() {
            return Err(RepoError::InvalidInput("application was not found".into()));
        }
        if !self
            .uri_available(&application.uri, Some(application.id))
            .await?
        {
            return Err(RepoError::InvalidInput(
                "application URI already exists".into(),
            ));
        }
        application.updated_at = Utc::now();
        self.store
            .write(&path(application.id), &application)
            .await?;
        Ok(application)
    }

    async fn delete_application_by_id(&self, application_id: i64) -> RepoResult<()> {
        if self.find_by_id(application_id).await?.is_none() {
            return Err(RepoError::InvalidInput("application was not found".into()));
        }
        self.store.delete(&path(application_id)).await
    }
}

fn path(id: i64) -> String {
    format!("{APPLICATIONS_FOLDER}/{id}.json")
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, sync::Arc};

    use crate::repo::ApplicationRepo;
    use file_system::{FileSystem, InMemoryStorage, MemoryTransport, PeerCodec};

    use super::FsApplicationRepo;

    #[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
    struct Peer;

    impl PeerCodec for Peer {
        type Error = Infallible;
        type PeerId = Self;

        fn encode(_: &Self::PeerId) -> Vec<u8> {
            Vec::new()
        }

        fn decode(_: &[u8]) -> Result<Self::PeerId, Self::Error> {
            Ok(Self)
        }
    }

    #[tokio::test]
    async fn persists_and_enforces_unique_uris() {
        let (transport, _) = MemoryTransport::pair(Peer, Peer);
        let file_system: Arc<FileSystem<InMemoryStorage, Peer, MemoryTransport<Peer>>> = Arc::new(
            FileSystem::new(InMemoryStorage::new(), Peer, transport)
                .await
                .unwrap(),
        );
        let repo = FsApplicationRepo::new(file_system);
        let mut application = repo
            .create_application("IDP".into(), "idp".into(), None)
            .await
            .unwrap();

        assert!(
            repo.create_application("duplicate".into(), "idp".into(), None)
                .await
                .is_err()
        );
        application.name = "Local IDP".into();
        assert_eq!(
            repo.update_application(application).await.unwrap().name,
            "Local IDP"
        );
    }
}
