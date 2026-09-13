use idp_model::{contract::TrustedDevice, model::Device};

use crate::ManagementResult;

pub trait DeviceRepo {
    fn create(
        &self,
        name: String,
        public_key: String,
        address: String,
        enrollment_code_hash: Vec<u8>,
        enrollment_expires_at: i64,
    ) -> impl Future<Output = ManagementResult<Device>>;

    fn create_pairing(
        &self,
        name: String,
        public_key: String,
        address: String,
        accepting_public_key: String,
    ) -> impl Future<Output = ManagementResult<Device>>;

    fn pending_pairing(
        &self,
        device_id: i64,
    ) -> impl Future<Output = ManagementResult<Option<(Device, String)>>>;

    fn approve_pairing(
        &self,
        device_id: i64,
    ) -> impl Future<Output = ManagementResult<Option<Device>>>;

    fn has_any(&self) -> impl Future<Output = ManagementResult<bool>>;

    fn list(&self) -> impl Future<Output = ManagementResult<Vec<Device>>>;

    fn list_approved(&self) -> impl Future<Output = ManagementResult<Vec<TrustedDevice>>>;

    fn are_approved(
        &self,
        local_public_key: &str,
        remote_public_key: &str,
    ) -> impl Future<Output = ManagementResult<bool>> + Send;

    fn approve(
        &self,
        device_id: i64,
        enrollment_code_hash: &[u8],
    ) -> impl Future<Output = ManagementResult<Option<Device>>>;

    fn rename(
        &self,
        device_id: i64,
        name: String,
    ) -> impl Future<Output = ManagementResult<Option<Device>>>;

    fn revoke(
        &self,
        device_id: i64,
        protected_public_key: &str,
    ) -> impl Future<Output = ManagementResult<bool>>;

    fn revoke_self(&self, public_key: &str) -> impl Future<Output = ManagementResult<bool>>;
}
