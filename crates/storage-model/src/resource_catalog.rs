use alloc::string::String;

use crate::StorageNamespace;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceKind {
    Database,
    FileSystem,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceIdentity {
    pub kind: ResourceKind,
    pub id: String,
}

pub trait ResourceCatalog {
    fn contains(
        &self,
        namespace: &dyn StorageNamespace,
        resource: &ResourceIdentity,
    ) -> Result<bool, String>;
}
