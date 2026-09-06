use lidp_model::contract::TrustedDevice;

use crate::repo::RepoResult;

pub trait UserDeviceRepo {
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
}
