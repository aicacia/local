use std::{collections::BTreeSet, sync::Arc};

use chrono::{DateTime, Utc};
use file_system::{FileSystem, PeerCodec, Storage, Transport};
use idp_model::{contract::CodeChallengeMethod, model::OAuth2AuthorizationCode};

use crate::{
    generate_random_string,
    repo::{OAuth2AuthorizationCodeRepo, RepoError, RepoResult},
};

use super::JsonStore;

const AUTHORIZATION_CODES_FOLDER: &str = "idp/oauth2-authorization-codes";

pub struct FsOAuth2AuthorizationCodeRepo<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>>
{
    store: JsonStore<S, C, T>,
}

impl<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>> Clone
    for FsOAuth2AuthorizationCodeRepo<S, C, T>
{
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
        }
    }
}

impl<S, C, T> FsOAuth2AuthorizationCodeRepo<S, C, T>
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

    async fn authorization_codes(&self) -> RepoResult<Vec<OAuth2AuthorizationCode>> {
        let paths = self.store.list(AUTHORIZATION_CODES_FOLDER).await?;
        let mut codes = Vec::with_capacity(paths.len());
        for path in paths {
            codes.push(self.store.read(&path).await?);
        }
        Ok(codes)
    }

    async fn next_id(&self, codes: &[OAuth2AuthorizationCode]) -> RepoResult<i64> {
        let ids = codes.iter().map(|code| code.id).collect::<BTreeSet<_>>();
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

impl<S, C, T> OAuth2AuthorizationCodeRepo for FsOAuth2AuthorizationCodeRepo<S, C, T>
where
    S: Storage + Send + Sync + 'static,
    S::Error: std::fmt::Display + Send + 'static,
    C: PeerCodec + Send + 'static,
    C::Error: Send + 'static,
    C::PeerId: Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: 'static,
{
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
        let codes = self.authorization_codes().await?;
        let existing_codes = codes.iter().map(|code| &code.code).collect::<BTreeSet<_>>();
        let code = loop {
            let code = generate_random_string::<32>();
            if !existing_codes.contains(&code) {
                break code;
            }
        };
        let now = Utc::now();
        let authorization_code = OAuth2AuthorizationCode {
            id: self.next_id(&codes).await?,
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
            consumed_at: None,
            created_at: now,
            updated_at: now,
        };
        self.store
            .write(&path(authorization_code.id), &authorization_code)
            .await?;
        Ok(authorization_code)
    }

    async fn find_authorization_code_by_code(
        &self,
        code: &str,
    ) -> RepoResult<Option<OAuth2AuthorizationCode>> {
        Ok(self
            .authorization_codes()
            .await?
            .into_iter()
            .find(|authorization_code| authorization_code.code == code))
    }

    async fn consume_authorization_code(
        &self,
        id: i64,
        _consumed_at: DateTime<Utc>,
    ) -> RepoResult<()> {
        self.store.delete(&path(id)).await
    }
}

fn path(id: i64) -> String {
    format!("{AUTHORIZATION_CODES_FOLDER}/{id}.json")
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, sync::Arc};

    use chrono::{Duration, Utc};
    use file_system::{FileSystem, InMemoryStorage, MemoryTransport, PeerCodec};

    use crate::repo::OAuth2AuthorizationCodeRepo;

    use super::FsOAuth2AuthorizationCodeRepo;

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
    async fn persists_and_consumes_codes_once() {
        let (transport, _) = MemoryTransport::pair(Peer, Peer);
        let file_system: Arc<FileSystem<InMemoryStorage, Peer, MemoryTransport<Peer>>> = Arc::new(
            FileSystem::new(InMemoryStorage::new(), Peer, transport)
                .await
                .unwrap(),
        );
        let repo = FsOAuth2AuthorizationCodeRepo::new(file_system);
        let code = repo
            .create_authorization_code(
                "client".into(),
                1,
                "https://client.example/callback".into(),
                vec!["openid".into()],
                None,
                None,
                None,
                None,
                Utc::now() + Duration::minutes(1),
            )
            .await
            .unwrap();

        assert_eq!(
            repo.find_authorization_code_by_code(&code.code)
                .await
                .unwrap()
                .unwrap()
                .id,
            code.id
        );

        let (first, second) = tokio::join!(
            repo.consume_authorization_code(code.id, Utc::now()),
            repo.consume_authorization_code(code.id, Utc::now()),
        );
        assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
        assert!(
            repo.find_authorization_code_by_code(&code.code)
                .await
                .unwrap()
                .is_none()
        );
    }
}
