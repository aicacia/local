use lidp_model::{contract::TrustedDevice, model::UserDevice};

use crate::repo::RepoResult;

pub trait UserDeviceRepo {
    fn create(
        &self,
        user_id: i64,
        name: String,
        public_key: String,
        address: String,
        enrollment_code_hash: Vec<u8>,
        enrollment_expires_at: i64,
    ) -> impl Future<Output = RepoResult<UserDevice>>;

    fn create_pairing_invitation(
        &self,
        user_id: i64,
        initiating_public_key: String,
        secret_hash: Vec<u8>,
        expires_at: i64,
    ) -> impl Future<Output = RepoResult<Option<i64>>>;

    fn redeem_pairing_invitation(
        &self,
        secret_hash: &[u8],
        name: String,
        public_key: String,
        address: String,
    ) -> impl Future<Output = RepoResult<Option<(i64, UserDevice)>>>;

    fn pending_pairing(
        &self,
        user_id: i64,
        device_id: i64,
    ) -> impl Future<Output = RepoResult<Option<(i64, UserDevice, String)>>>;

    fn approve_pairing(
        &self,
        user_id: i64,
        device_id: i64,
        invitation_id: i64,
    ) -> impl Future<Output = RepoResult<Option<UserDevice>>>;

    fn has_any_by_user_id(&self, user_id: i64) -> impl Future<Output = RepoResult<bool>>;

    fn list_by_user_id(&self, user_id: i64) -> impl Future<Output = RepoResult<Vec<UserDevice>>>;

    fn list_approved_by_user_id(
        &self,
        user_id: i64,
    ) -> impl Future<Output = RepoResult<Vec<TrustedDevice>>>;

    fn are_approved_by_user_id(
        &self,
        user_id: i64,
        local_public_key: &str,
        remote_public_key: &str,
    ) -> impl Future<Output = RepoResult<bool>>;

    fn has_approved_by_user_id(&self, user_id: i64) -> impl Future<Output = RepoResult<bool>>;

    fn approve(
        &self,
        user_id: i64,
        device_id: i64,
        enrollment_code_hash: &[u8],
    ) -> impl Future<Output = RepoResult<Option<UserDevice>>>;

    fn rename(
        &self,
        user_id: i64,
        device_id: i64,
        name: String,
    ) -> impl Future<Output = RepoResult<Option<UserDevice>>>;

    fn revoke(&self, user_id: i64, device_id: i64) -> impl Future<Output = RepoResult<bool>>;
}
