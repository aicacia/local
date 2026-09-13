#[cfg(not(feature = "std"))]
use alloc::{string::String, vec::Vec};

use serde::{Deserialize, Serialize};

pub const GLOBAL_IDENTITY_MANIFEST_VERSION: u8 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GlobalIdentityManifest {
    pub version: u8,
    pub revision: String,
    pub records: Vec<GlobalIdentityRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GlobalIdentityRecord {
    pub path: String,
    pub hash: String,
}

impl GlobalIdentityManifest {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.version == GLOBAL_IDENTITY_MANIFEST_VERSION
            && valid_component(&self.revision)
            && !self.records.is_empty()
            && self
                .records
                .windows(2)
                .all(|records| records[0].path < records[1].path)
            && self.records.iter().all(valid_record)
    }
}

fn valid_record(record: &GlobalIdentityRecord) -> bool {
    record.path.starts_with("records/")
        && record.path.strip_prefix("records/").is_some_and(valid_path)
        && record.hash.len() == 64
        && record.hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_path(path: &str) -> bool {
    !path.is_empty() && path.split('/').all(valid_component)
}

fn valid_component(component: &str) -> bool {
    !component.is_empty() && component != "." && component != ".."
}

#[cfg(test)]
mod tests {
    use super::{GLOBAL_IDENTITY_MANIFEST_VERSION, GlobalIdentityManifest, GlobalIdentityRecord};

    fn manifest() -> GlobalIdentityManifest {
        GlobalIdentityManifest {
            version: GLOBAL_IDENTITY_MANIFEST_VERSION,
            revision: "revision".to_owned(),
            records: vec![GlobalIdentityRecord {
                path: "records/idp/devices/device.json".to_owned(),
                hash: "0".repeat(64),
            }],
        }
    }

    #[test]
    fn validates_canonical_manifests() {
        assert!(manifest().is_valid());
    }

    #[test]
    fn rejects_invalid_paths_hashes_and_ordering() {
        let mut value = manifest();
        value.records[0].path = "../device.json".to_owned();
        assert!(!value.is_valid());

        let mut value = manifest();
        value.records[0].hash = "invalid".to_owned();
        assert!(!value.is_valid());

        let mut value = manifest();
        value.records.push(GlobalIdentityRecord {
            path: "records/a.json".to_owned(),
            hash: "0".repeat(64),
        });
        assert!(!value.is_valid());
    }
}
