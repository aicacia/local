#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
#[serde(tag = "type")]
pub enum AuthorizationDetail {
    #[serde(rename = "storage")]
    Storage(StorageAuthorizationDetail),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
pub struct StorageAuthorizationDetail {
    pub folder: String,
    pub actions: Vec<StorageAuthorizationAction>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
#[serde(rename_all = "lowercase")]
pub enum StorageAuthorizationAction {
    Read,
    Write,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_storage_detail_with_rfc_type() {
        let detail = AuthorizationDetail::Storage(StorageAuthorizationDetail {
            folder: "documents".to_string(),
            actions: vec![StorageAuthorizationAction::Read],
        });

        assert_eq!(
            serde_json::to_string(&detail).unwrap(),
            r#"{"type":"storage","folder":"documents","actions":["read"]}"#
        );
    }
}
