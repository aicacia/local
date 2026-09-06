use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use lidp_model::{
    contract::{
        DeviceEnrollment, DeviceEnrollmentRequest, DeviceInfo, ErrorCode, ErrorResponse,
        ErrorResponseResult, UpdateDeviceRequest,
    },
    model::UserDevice,
};
use sha2::{Digest, Sha256};

use crate::repo::UserDeviceRepo;

const APPROVAL_CODE_BYTES: usize = 32;
const APPROVAL_CODE_TTL_SECONDS: i64 = 300;

pub struct DeviceEnrollmentService<R> {
    repo: Arc<R>,
}

impl<R> DeviceEnrollmentService<R>
where
    R: UserDeviceRepo,
{
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }

    pub async fn enroll(
        &self,
        user_id: i64,
        request: DeviceEnrollmentRequest,
    ) -> ErrorResponseResult<DeviceEnrollment> {
        validate_enrollment(&request)?;
        let approval_code = generate_approval_code()?;
        let device = self
            .repo
            .create(
                user_id,
                request.name,
                request.public_key,
                request.address,
                hash_approval_code(&approval_code),
                now() + APPROVAL_CODE_TTL_SECONDS,
            )
            .await
            .map_err(ErrorResponse::from)?;
        Ok(DeviceEnrollment {
            id: device.id,
            state: device.state,
            approval_code: (device.state == lidp_model::contract::UserDeviceState::Pending)
                .then_some(approval_code),
        })
    }

    pub async fn approve(
        &self,
        user_id: i64,
        device_id: i64,
        approval_code: &str,
    ) -> ErrorResponseResult<DeviceInfo> {
        if !self
            .repo
            .has_approved_by_user_id(user_id)
            .await
            .map_err(ErrorResponse::from)?
        {
            return Err(ErrorResponse::new(ErrorCode::AccessDenied));
        }
        let device = self
            .repo
            .approve(user_id, device_id, &hash_approval_code(approval_code))
            .await
            .map_err(ErrorResponse::from)?
            .ok_or_else(|| ErrorResponse::new(ErrorCode::AccessDenied))?;
        Ok(device.into())
    }

    pub async fn list(&self, user_id: i64) -> ErrorResponseResult<Vec<DeviceInfo>> {
        self.repo
            .list_by_user_id(user_id)
            .await
            .map(|devices| devices.into_iter().map(UserDevice::into).collect())
            .map_err(ErrorResponse::from)
    }

    pub async fn rename(
        &self,
        user_id: i64,
        device_id: i64,
        request: UpdateDeviceRequest,
    ) -> ErrorResponseResult<DeviceInfo> {
        validate_name(&request.name)?;
        self.repo
            .rename(user_id, device_id, request.name)
            .await
            .map_err(ErrorResponse::from)?
            .map(Into::into)
            .ok_or_else(|| ErrorResponse::new(ErrorCode::NotFound))
    }

    pub async fn revoke(&self, user_id: i64, device_id: i64) -> ErrorResponseResult<()> {
        self.repo
            .revoke(user_id, device_id)
            .await
            .map_err(ErrorResponse::from)?
            .then_some(())
            .ok_or_else(|| ErrorResponse::new(ErrorCode::NotFound))
    }
}

fn validate_enrollment(request: &DeviceEnrollmentRequest) -> ErrorResponseResult<()> {
    validate_name(&request.name)?;
    if !valid_value(&request.public_key) || !valid_value(&request.address) {
        return Err(ErrorResponse::new(ErrorCode::InvalidRequest));
    }
    Ok(())
}

fn validate_name(name: &str) -> ErrorResponseResult<()> {
    valid_value(name)
        .then_some(())
        .ok_or_else(|| ErrorResponse::new(ErrorCode::InvalidRequest))
}

fn valid_value(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 512
}

fn generate_approval_code() -> ErrorResponseResult<String> {
    let mut bytes = [0_u8; APPROVAL_CODE_BYTES];
    getrandom::fill(&mut bytes).map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn hash_approval_code(code: &str) -> Vec<u8> {
    Sha256::digest(code.as_bytes()).to_vec()
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before Unix epoch")
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use lidp_model::contract::DeviceEnrollmentRequest;

    use super::validate_enrollment;

    #[test]
    fn rejects_empty_device_identity_fields() {
        assert!(
            validate_enrollment(&DeviceEnrollmentRequest {
                name: "device".into(),
                public_key: " ".into(),
                address: "address".into(),
            })
            .is_err()
        );
    }
}
