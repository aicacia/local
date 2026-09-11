use key::{DerivationPath, DerivedKey, MasterKey};

use crate::repo::{RepoError, RepoResult};

pub trait PrivateKeyRepo {
    fn load(
        &self,
        namespace: &str,
        derivation_path: &DerivationPath,
    ) -> RepoResult<Option<DerivedKey>>;
    fn store(&self, namespace: &str, derived_key: &DerivedKey) -> RepoResult<()>;
    fn delete(&self, namespace: &str, derivation_path: &DerivationPath) -> RepoResult<()>;

    fn ensure_master_key(&self, namespace: &str) -> RepoResult<DerivedKey> {
        self.ensure_master_key_with_passphrase(namespace, "")
    }

    fn ensure_master_key_with_passphrase(
        &self,
        namespace: &str,
        passphrase: &str,
    ) -> RepoResult<DerivedKey> {
        let derivation_path = DerivationPath::default();
        match self.load(namespace, &derivation_path)? {
            Some(derived_key) => Ok(derived_key),
            None => {
                let entropy = MasterKey::entropy()?;
                let master_key = MasterKey::from_entropy_with_passphrase(entropy, passphrase)?;
                let derived_key = master_key.into();
                self.store(namespace, &derived_key)?;
                Ok(derived_key)
            }
        }
    }

    /// ensures the key exists for the given namespace and derivation path,
    /// if it does not, it checks the parent, once it finds a key in the parent chain
    /// it will try to derive the parent keys in the chain until it creates the key
    /// for the given derivation path
    fn ensure_derivation_path(
        &self,
        namespace: &str,
        derivation_path: DerivationPath,
    ) -> RepoResult<DerivedKey> {
        match self.load(namespace, &derivation_path)? {
            Some(derived_key) => Ok(derived_key),
            None => {
                let parent_path = derivation_path.parent().ok_or_else(|| {
                    RepoError::InvalidInput(
                        "Cannot ensure parent for root derivation path".to_string(),
                    )
                })?;

                let parent_key = self.ensure_derivation_path(namespace, parent_path)?;
                let derived_key = parent_key.derive_from_derivation_path(derivation_path)?;

                self.store(namespace, &derived_key)?;

                Ok(derived_key)
            }
        }
    }
}
