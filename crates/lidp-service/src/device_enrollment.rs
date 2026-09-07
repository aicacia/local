use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use iroh::{EndpointId, Signature};
use lidp_model::{
    contract::{
        DeviceEnrollment, DeviceEnrollmentRequest, DeviceInfo, DevicePairingApprovalRequest,
        DevicePairingInvitation, DevicePairingInvitationRequest, DevicePairingRedemptionRequest,
        ErrorCode, ErrorResponse, ErrorResponseResult, UpdateDeviceRequest,
        device_pairing_approval_payload,
    },
    model::UserDevice,
};
use sha2::{Digest, Sha256};

use crate::repo::UserDeviceRepo;

const PAIRING_SECRET_BYTES: usize = 32;
const PAIRING_TTL_SECONDS: i64 = 300;

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
        if self
            .repo
            .has_any_by_user_id(user_id)
            .await
            .map_err(ErrorResponse::from)?
        {
            return Err(ErrorResponse::new(ErrorCode::AccessDenied));
        }
        let device = self
            .repo
            .create(
                user_id,
                request.name,
                request.public_key,
                request.address,
                Vec::new(),
                0,
            )
            .await
            .map_err(ErrorResponse::from)?;
        Ok(DeviceEnrollment {
            id: device.id,
            state: device.state,
            approval_code: None,
        })
    }

    pub async fn create_pairing_invitation(
        &self,
        user_id: i64,
        request: DevicePairingInvitationRequest,
    ) -> ErrorResponseResult<DevicePairingInvitation> {
        validate_public_key(&request.initiating_public_key)?;
        let secret = generate_secret()?;
        let expires_at = now() + PAIRING_TTL_SECONDS;
        let id = self
            .repo
            .create_pairing_invitation(
                user_id,
                request.initiating_public_key,
                hash_secret(&secret),
                expires_at,
            )
            .await
            .map_err(ErrorResponse::from)?
            .ok_or_else(|| ErrorResponse::new(ErrorCode::AccessDenied))?;
        Ok(DevicePairingInvitation {
            id,
            secret,
            expires_at,
        })
    }

    pub async fn redeem_pairing_invitation(
        &self,
        request: DevicePairingRedemptionRequest,
    ) -> ErrorResponseResult<DeviceEnrollment> {
        validate_enrollment(&DeviceEnrollmentRequest {
            name: request.name.clone(),
            public_key: request.public_key.clone(),
            address: request.address.clone(),
        })?;
        let (_, device) = self
            .repo
            .redeem_pairing_invitation(
                &hash_secret(&request.secret),
                request.name,
                request.public_key,
                request.address,
            )
            .await
            .map_err(ErrorResponse::from)?
            .ok_or_else(|| ErrorResponse::new(ErrorCode::AccessDenied))?;
        Ok(DeviceEnrollment {
            id: device.id,
            state: device.state,
            approval_code: None,
        })
    }

    pub async fn approve_pairing(
        &self,
        user_id: i64,
        device_id: i64,
        request: DevicePairingApprovalRequest,
    ) -> ErrorResponseResult<DeviceInfo> {
        let (invitation_id, device, initiating_public_key) = self
            .repo
            .pending_pairing(user_id, device_id)
            .await
            .map_err(ErrorResponse::from)?
            .ok_or_else(|| ErrorResponse::new(ErrorCode::AccessDenied))?;
        verify_signature(
            &initiating_public_key,
            &device_pairing_approval_payload(
                invitation_id,
                device.id,
                &device.name,
                &device.public_key,
                &device.address,
            ),
            &request.signature,
        )?;
        self.repo
            .approve_pairing(user_id, device_id, invitation_id)
            .await
            .map_err(ErrorResponse::from)?
            .map(Into::into)
            .ok_or_else(|| ErrorResponse::new(ErrorCode::AccessDenied))
    }

    pub async fn pairing_approval_payload(
        &self,
        user_id: i64,
        device_id: i64,
    ) -> ErrorResponseResult<String> {
        let (invitation_id, device, _) = self
            .repo
            .pending_pairing(user_id, device_id)
            .await
            .map_err(ErrorResponse::from)?
            .ok_or_else(|| ErrorResponse::new(ErrorCode::AccessDenied))?;
        Ok(device_pairing_approval_payload(
            invitation_id,
            device.id,
            &device.name,
            &device.public_key,
            &device.address,
        ))
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
    validate_public_key(&request.public_key)?;
    valid_value(&request.address)
        .then_some(())
        .ok_or_else(|| ErrorResponse::new(ErrorCode::InvalidRequest))
}

fn validate_name(name: &str) -> ErrorResponseResult<()> {
    valid_value(name)
        .then_some(())
        .ok_or_else(|| ErrorResponse::new(ErrorCode::InvalidRequest))
}

fn validate_public_key(public_key: &str) -> ErrorResponseResult<()> {
    (valid_value(public_key) && public_key.parse::<EndpointId>().is_ok())
        .then_some(())
        .ok_or_else(|| ErrorResponse::new(ErrorCode::InvalidRequest))
}

fn valid_value(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 512
}

fn generate_secret() -> ErrorResponseResult<String> {
    let mut bytes = [0_u8; PAIRING_SECRET_BYTES];
    getrandom::fill(&mut bytes).map_err(|_| ErrorResponse::new(ErrorCode::ServerError))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn hash_secret(secret: &str) -> Vec<u8> {
    Sha256::digest(secret.as_bytes()).to_vec()
}

fn verify_signature(public_key: &str, payload: &str, signature: &str) -> ErrorResponseResult<()> {
    let public_key = public_key
        .parse::<EndpointId>()
        .map_err(|_| ErrorResponse::new(ErrorCode::AccessDenied))?;
    let signature = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| ErrorResponse::new(ErrorCode::AccessDenied))?;
    let signature = Signature::try_from(signature.as_slice())
        .map_err(|_| ErrorResponse::new(ErrorCode::AccessDenied))?;
    public_key
        .verify(payload.as_bytes(), &signature)
        .map_err(|_| ErrorResponse::new(ErrorCode::AccessDenied))
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before Unix epoch")
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use iroh::SecretKey;

    use super::{device_pairing_approval_payload, verify_signature};

    #[test]
    fn verifies_only_the_bound_signer() {
        let signer = SecretKey::generate();
        let payload = device_pairing_approval_payload(1, 2, "device", "key", "address");
        let signature = URL_SAFE_NO_PAD.encode(signer.sign(payload.as_bytes()).to_bytes());

        assert!(verify_signature(&signer.public().to_string(), &payload, &signature).is_ok());
        assert!(
            verify_signature(
                &SecretKey::generate().public().to_string(),
                &payload,
                &signature
            )
            .is_err()
        );
    }
}
