use std::sync::Arc;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use idp_model::{
    contract::{
        DeviceEnrollment, DeviceEnrollmentRequest, DeviceInfo, DevicePairingApprovalRequest,
        DevicePairingRequest, ErrorCode, ErrorResponse, ErrorResponseResult, UpdateDeviceRequest,
        device_pairing_approval_payload,
    },
    model::Device,
};
use iroh::{EndpointId, Signature};

use crate::DeviceRepo;

pub struct DeviceEnrollmentService<R> {
    repo: Arc<R>,
}

impl<R> DeviceEnrollmentService<R>
where
    R: DeviceRepo,
{
    pub fn new(repo: Arc<R>) -> Self {
        Self { repo }
    }

    pub async fn enroll(
        &self,
        owner_subject: String,
        request: DeviceEnrollmentRequest,
    ) -> ErrorResponseResult<DeviceEnrollment> {
        if owner_subject.trim().is_empty() {
            return Err(ErrorResponse::new(ErrorCode::AccessDenied));
        }
        validate_enrollment(&request)?;
        if self.repo.has_any().await.map_err(ErrorResponse::from)? {
            return Err(ErrorResponse::new(ErrorCode::AccessDenied));
        }
        let device = self
            .repo
            .create(
                owner_subject,
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

    pub async fn request_pairing(
        &self,
        request: DevicePairingRequest,
    ) -> ErrorResponseResult<DeviceEnrollment> {
        validate_enrollment(&DeviceEnrollmentRequest {
            name: request.name.clone(),
            public_key: request.public_key.clone(),
            address: request.address.clone(),
        })?;
        validate_public_key(&request.accepting_public_key)?;
        let device = self
            .repo
            .create_pairing(
                request.name,
                request.public_key,
                request.address,
                request.accepting_public_key,
            )
            .await
            .map_err(ErrorResponse::from)?;
        Ok(DeviceEnrollment {
            id: device.id,
            state: device.state,
            approval_code: None,
        })
    }

    pub async fn approve_pairing(
        &self,
        device_id: idp_model::model::Id,
        request: DevicePairingApprovalRequest,
    ) -> ErrorResponseResult<DeviceInfo> {
        let (device, accepting_public_key) = self
            .repo
            .pending_pairing(device_id)
            .await
            .map_err(ErrorResponse::from)?
            .ok_or_else(|| ErrorResponse::new(ErrorCode::AccessDenied))?;
        verify_signature(
            &accepting_public_key,
            &device_pairing_approval_payload(
                device.id,
                &device.name,
                &device.public_key,
                &device.address,
            ),
            &request.signature,
        )?;
        self.repo
            .approve_pairing(device_id)
            .await
            .map_err(ErrorResponse::from)?
            .map(Into::into)
            .ok_or_else(|| ErrorResponse::new(ErrorCode::AccessDenied))
    }

    pub async fn pairing_approval_payload(
        &self,
        device_id: idp_model::model::Id,
    ) -> ErrorResponseResult<String> {
        let (device, _) = self
            .repo
            .pending_pairing(device_id)
            .await
            .map_err(ErrorResponse::from)?
            .ok_or_else(|| ErrorResponse::new(ErrorCode::AccessDenied))?;
        Ok(device_pairing_approval_payload(
            device.id,
            &device.name,
            &device.public_key,
            &device.address,
        ))
    }

    pub async fn list(&self, owner_subject: &str) -> ErrorResponseResult<Vec<DeviceInfo>> {
        self.repo
            .list_owned(owner_subject)
            .await
            .map(|devices| devices.into_iter().map(Device::into).collect())
            .map_err(ErrorResponse::from)
    }

    pub async fn rename(
        &self,
        owner_subject: &str,
        device_id: idp_model::model::Id,
        request: UpdateDeviceRequest,
    ) -> ErrorResponseResult<DeviceInfo> {
        validate_name(&request.name)?;
        self.repo
            .rename(owner_subject, device_id, request.name)
            .await
            .map_err(ErrorResponse::from)?
            .map(Into::into)
            .ok_or_else(|| ErrorResponse::new(ErrorCode::NotFound))
    }

    pub async fn revoke(
        &self,
        owner_subject: &str,
        device_id: idp_model::model::Id,
        protected_public_key: &str,
    ) -> ErrorResponseResult<()> {
        self.repo
            .revoke(owner_subject, device_id, protected_public_key)
            .await
            .map_err(ErrorResponse::from)?
            .then_some(())
            .ok_or_else(|| ErrorResponse::new(ErrorCode::NotFound))
    }

    pub async fn revoke_self(&self, public_key: &str) -> ErrorResponseResult<()> {
        self.repo
            .revoke_self(public_key)
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
    if !valid_value(public_key) {
        return Err(ErrorResponse::new(ErrorCode::InvalidRequest));
    }
    let endpoint_id = public_key
        .parse::<EndpointId>()
        .map_err(|_| ErrorResponse::new(ErrorCode::InvalidRequest))?;
    (endpoint_id.to_string() == public_key)
        .then_some(())
        .ok_or_else(|| ErrorResponse::new(ErrorCode::InvalidRequest))
}

fn valid_value(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 512
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

#[cfg(test)]
mod tests {
    use iroh::{EndpointId, SecretKey};

    use super::validate_public_key;

    #[test]
    fn public_keys_must_use_the_canonical_endpoint_id_encoding() {
        let canonical = SecretKey::generate().public().to_string();
        assert!(validate_public_key(&canonical).is_ok());
        assert!(validate_public_key("not-an-endpoint-id").is_err());

        let alternate_case = canonical.to_ascii_uppercase();
        if alternate_case != canonical && alternate_case.parse::<EndpointId>().is_ok() {
            assert!(validate_public_key(&alternate_case).is_err());
        }
    }
}
