use idp_model::model::{Id, OAuth2UserConsent};

use crate::repo::RepoResult;

pub trait OAuth2UserConsentRepo {
    fn upsert_user_consent(
        &self,
        user_id: Id,
        client_id: &str,
        redirect_uri: &str,
        scope: &str,
    ) -> impl Future<Output = RepoResult<OAuth2UserConsent>>;

    fn find_user_consent(
        &self,
        user_id: Id,
        client_id: &str,
        redirect_uri: &str,
        scope: &str,
    ) -> impl Future<Output = RepoResult<Option<OAuth2UserConsent>>>;

    fn list_user_consents(
        &self,
        user_id: Id,
        offset: u32,
        limit: u32,
    ) -> impl Future<Output = RepoResult<Vec<OAuth2UserConsent>>>;

    fn find_user_consent_by_id(
        &self,
        consent_id: Id,
    ) -> impl Future<Output = RepoResult<Option<OAuth2UserConsent>>>;

    fn delete_user_consent_by_id(&self, consent_id: Id) -> impl Future<Output = RepoResult<()>>;
}
