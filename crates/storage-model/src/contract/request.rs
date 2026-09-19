use alloc::{string::String, vec::Vec};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StorageRequest {
    Read { path: String },
    Write { path: String, content: Vec<u8> },
    Append { path: String, content: Vec<u8> },
    Delete { path: String },
    CreateDir { path: String },
    Rename { from: String, to: String },
    Entry { path: String },
    List { path: String },
}
