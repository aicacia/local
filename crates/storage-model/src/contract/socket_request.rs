use alloc::string::String;

use serde::{Deserialize, Serialize};

use super::StorageRequest;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StorageSocketRequest {
    Authenticate { token: String },
    Request { request: StorageRequest },
}
