#[cfg(not(feature = "std"))]
use alloc::{format, string::String};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DevicePairingInvitationRequest {
    pub initiating_public_key: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DevicePairingInvitation {
    pub id: i64,
    pub secret: String,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DevicePairingRedemptionRequest {
    pub secret: String,
    pub name: String,
    pub public_key: String,
    pub address: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DevicePairingApprovalRequest {
    pub signature: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DevicePairingApprovalPayload {
    pub payload: String,
}

#[must_use]
pub fn device_pairing_approval_payload(
    invitation_id: i64,
    device_id: i64,
    name: &str,
    public_key: &str,
    address: &str,
) -> String {
    format!(
        "lidp-device-pairing-approval-v1\n{invitation_id}\n{device_id}\n{}\n{}\n{}\n{}\n{}\n{}",
        name.len(),
        name,
        public_key.len(),
        public_key,
        address.len(),
        address,
    )
}
