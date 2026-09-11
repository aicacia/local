#[cfg(not(feature = "std"))]
use alloc::{format, string::String};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct DevicePairingRequest {
    pub name: String,
    pub public_key: String,
    pub address: String,
    pub accepting_public_key: String,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct PairingAcceptance {
    pub accepting: bool,
}

#[must_use]
pub fn device_pairing_approval_payload(
    device_id: i64,
    name: &str,
    public_key: &str,
    address: &str,
) -> String {
    format!(
        "idp-device-pairing-approval-v1\n{device_id}\n{}\n{}\n{}\n{}\n{}\n{}",
        name.len(),
        name,
        public_key.len(),
        public_key,
        address.len(),
        address,
    )
}
