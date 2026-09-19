use alloc::{string::String, vec::Vec};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageEntry {
    pub name: String,
    pub hash: String,
    pub size: u64,
    pub local: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StorageResponse {
    Authenticated,
    Read { content: Vec<u8> },
    Written { entry: StorageEntry },
    Appended { entry: StorageEntry },
    Deleted,
    DirectoryCreated { entry: StorageEntry },
    Renamed,
    Entry { entry: StorageEntry },
    Listed { entries: Vec<StorageEntry> },
    Error { code: StorageErrorCode },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageErrorCode {
    InvalidRequest,
    OperationFailed,
}
