use std::{collections::BTreeSet, sync::Arc};

use chrono::Utc;
use file_system::{FileSystem, PeerCodec, Storage, Transport};
use idp_model::model::OAuth2UserConsent;

use crate::repo::{OAuth2UserConsentRepo, RepoError, RepoResult};

use super::JsonStore;

const USER_CONSENTS_FOLDER: &str = "idp/oauth2-user-consents";

pub struct FsOAuth2UserConsentRepo<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>> {
    store: JsonStore<S, C, T>,
}

impl<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>> Clone
    for FsOAuth2UserConsentRepo<S, C, T>
{
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
        }
    }
}

impl<S, C, T> FsOAuth2UserConsentRepo<S, C, T>
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

    async fn user_consents(&self) -> RepoResult<Vec<OAuth2UserConsent>> {
        let paths = self.store.list(USER_CONSENTS_FOLDER).await?;
        let mut consents = Vec::with_capacity(paths.len());
        for path in paths {
            consents.push(self.store.read(&path).await?);
        }
        Ok(consents)
    }

    async fn next_id(&self, consents: &[OAuth2UserConsent]) -> RepoResult<i64> {
        let ids = consents
            .iter()
            .map(|consent| consent.id)
            .collect::<BTreeSet<_>>();
        loop {
            let mut bytes = [0_u8; 8];
            getrandom::fill(&mut bytes).map_err(RepoError::other)?;
            let id = i64::from_be_bytes(bytes) & i64::MAX;
            if id != 0 && !ids.contains(&id) {
                return Ok(id);
            }
        }
    }
}

impl<S, C, T> OAuth2UserConsentRepo for FsOAuth2UserConsentRepo<S, C, T>
where
    S: Storage + Send + Sync + 'static,
    S::Error: std::fmt::Display + Send + 'static,
    C: PeerCodec + Send + 'static,
    C::Error: Send + 'static,
    C::PeerId: Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: 'static,
{
    async fn upsert_user_consent(
        &self,
        user_id: i64,
        client_id: &str,
        redirect_uri: &str,
        scope: &str,
    ) -> RepoResult<OAuth2UserConsent> {
        let consents = self.user_consents().await?;
        let now = Utc::now();
        let consent = consents
            .iter()
            .find(|consent| {
                consent.user_id == user_id
                    && consent.client_id == client_id
                    && consent.redirect_uri == redirect_uri
                    && consent.scope == scope
            })
            .map_or_else(
                || OAuth2UserConsent {
                    id: 0,
                    user_id,
                    client_id: client_id.into(),
                    redirect_uri: redirect_uri.into(),
                    scope: scope.into(),
                    created_at: now,
                    updated_at: now,
                },
                |existing| OAuth2UserConsent {
                    id: existing.id,
                    user_id,
                    client_id: client_id.into(),
                    redirect_uri: redirect_uri.into(),
                    scope: scope.into(),
                    created_at: existing.created_at,
                    updated_at: now,
                },
            );
        let consent = if consent.id == 0 {
            OAuth2UserConsent {
                id: self.next_id(&consents).await?,
                ..consent
            }
        } else {
            consent
        };
        self.store.write(&path(consent.id), &consent).await?;
        Ok(consent)
    }

    async fn find_user_consent(
        &self,
        user_id: i64,
        client_id: &str,
        redirect_uri: &str,
        scope: &str,
    ) -> RepoResult<Option<OAuth2UserConsent>> {
        Ok(self.user_consents().await?.into_iter().find(|consent| {
            consent.user_id == user_id
                && consent.client_id == client_id
                && consent.redirect_uri == redirect_uri
                && consent.scope == scope
        }))
    }

    async fn list_user_consents(
        &self,
        user_id: i64,
        offset: u32,
        limit: u32,
    ) -> RepoResult<Vec<OAuth2UserConsent>> {
        let mut consents = self
            .user_consents()
            .await?
            .into_iter()
            .filter(|consent| consent.user_id == user_id)
            .collect::<Vec<_>>();
        consents.sort_by_key(|consent| std::cmp::Reverse(consent.created_at));
        Ok(consents
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect())
    }

    async fn find_user_consent_by_id(
        &self,
        consent_id: i64,
    ) -> RepoResult<Option<OAuth2UserConsent>> {
        Ok(self
            .user_consents()
            .await?
            .into_iter()
            .find(|consent| consent.id == consent_id))
    }

    async fn delete_user_consent_by_id(&self, consent_id: i64) -> RepoResult<()> {
        self.store.delete(&path(consent_id)).await
    }
}

fn path(id: i64) -> String {
    format!("{USER_CONSENTS_FOLDER}/{id}.json")
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, sync::Arc};

    use file_system::{FileSystem, InMemoryStorage, MemoryTransport, PeerCodec};

    use crate::repo::OAuth2UserConsentRepo;

    use super::FsOAuth2UserConsentRepo;

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
    async fn upserts_lists_and_deletes_consents() {
        let (transport, _) = MemoryTransport::pair(Peer, Peer);
        let file_system: Arc<FileSystem<InMemoryStorage, Peer, MemoryTransport<Peer>>> = Arc::new(
            FileSystem::new(InMemoryStorage::new(), Peer, transport)
                .await
                .unwrap(),
        );
        let repo = FsOAuth2UserConsentRepo::new(file_system);
        let consent = repo
            .upsert_user_consent(1, "client", "https://client.example/callback", "openid")
            .await
            .unwrap();
        let updated = repo
            .upsert_user_consent(1, "client", "https://client.example/callback", "openid")
            .await
            .unwrap();

        assert_eq!(updated.id, consent.id);
        assert_eq!(
            repo.list_user_consents(1, 0, 10).await.unwrap()[0].id,
            updated.id
        );
        repo.delete_user_consent_by_id(consent.id).await.unwrap();
        assert!(
            repo.find_user_consent_by_id(consent.id)
                .await
                .unwrap()
                .is_none()
        );
    }
}
