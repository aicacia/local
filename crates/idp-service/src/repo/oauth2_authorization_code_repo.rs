use chrono::{DateTime, Utc};
use idp_model::{contract::CodeChallengeMethod, model::OAuth2AuthorizationCode};

use crate::repo::RepoResult;

pub trait OAuth2AuthorizationCodeRepo {
    fn create_authorization_code(
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
    ) -> impl Future<Output = RepoResult<OAuth2AuthorizationCode>>;

    fn find_authorization_code_by_code(
        &self,
        code: &str,
    ) -> impl Future<Output = RepoResult<Option<OAuth2AuthorizationCode>>>;

    fn consume_authorization_code(
        &self,
        id: i64,
        consumed_at: DateTime<Utc>,
    ) -> impl Future<Output = RepoResult<()>>;
}
