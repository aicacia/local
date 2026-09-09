#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use lidp_model::contract::DeviceEnrollmentRequest;

use crate::repo::{DeviceRepo, RepoResult};

pub async fn ensure_bootstrap_device<R>(
    devices: &R,
    device: DeviceEnrollmentRequest,
) -> RepoResult<()>
where
    R: DeviceRepo,
{
    if devices.has_any().await? {
        return Ok(());
    }
    devices
        .create(
            device.name,
            device.public_key,
            device.address,
            Vec::new(),
            0,
        )
        .await?;
    Ok(())
}
