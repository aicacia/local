#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

use idp_model::contract::DeviceEnrollmentRequest;

use management_service::DeviceRepo;

use crate::BootstrapResult;

pub async fn ensure_bootstrap_device<R>(
    devices: &R,
    device: DeviceEnrollmentRequest,
) -> BootstrapResult<()>
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
