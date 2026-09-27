#[cfg(not(feature = "std"))]
use alloc::string::String;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GlobalIdentityBootstrapGrant {
    pub grant_id: String,
    pub vault_id_hash: String,
    pub initiating_public_key: String,
    pub accepting_public_key: String,
    pub nonce: String,
    pub expires_at: i64,
}

impl GlobalIdentityBootstrapGrant {
    #[must_use]
    pub fn valid_for(
        &self,
        vault_id_hash: &str,
        initiating_public_key: &str,
        accepting_public_key: &str,
        nonce: &str,
        now: i64,
    ) -> bool {
        !self.grant_id.is_empty()
            && self.vault_id_hash == vault_id_hash
            && self.initiating_public_key == initiating_public_key
            && self.accepting_public_key == accepting_public_key
            && !self.nonce.is_empty()
            && self.nonce == nonce
            && self.expires_at > now
    }
}

#[cfg(test)]
mod tests {
    use super::GlobalIdentityBootstrapGrant;

    fn grant() -> GlobalIdentityBootstrapGrant {
        GlobalIdentityBootstrapGrant {
            grant_id: "grant".to_owned(),
            vault_id_hash: "vault".to_owned(),
            initiating_public_key: "joining".to_owned(),
            accepting_public_key: "approved".to_owned(),
            nonce: "nonce".to_owned(),
            expires_at: 101,
        }
    }

    #[test]
    fn requires_the_complete_bootstrap_binding() {
        assert!(grant().valid_for("vault", "joining", "approved", "nonce", 100));
        assert!(!grant().valid_for("other", "joining", "approved", "nonce", 100));
        assert!(!grant().valid_for("vault", "other", "approved", "nonce", 100));
        assert!(!grant().valid_for("vault", "joining", "other", "nonce", 100));
        assert!(!grant().valid_for("vault", "joining", "approved", "other", 100));
        assert!(!grant().valid_for("vault", "joining", "approved", "nonce", 101));
    }
}
