#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use lidp_model::contract::DeviceEnrollmentRequest;

use crate::repo::{RepoResult, UserDeviceRepo};

pub async fn ensure_bootstrap_device<R>(
    user_id: i64,
    user_devices: &R,
    device: DeviceEnrollmentRequest,
) -> RepoResult<()>
where
    R: UserDeviceRepo,
{
    if user_devices.has_any_by_user_id(user_id).await? {
        return Ok(());
    }
    user_devices
        .create(
            user_id,
            device.name,
            device.public_key,
            device.address,
            Vec::new(),
            0,
        )
        .await?;
    Ok(())
}
