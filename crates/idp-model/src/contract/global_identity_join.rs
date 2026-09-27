#[cfg(not(feature = "std"))]
use alloc::string::String;

use serde::{Deserialize, Serialize};

use super::GlobalIdentityBootstrapGrant;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GlobalIdentityJoinOffer {
    pub device_name: String,
    pub joining_endpoint_addr: String,
    pub joining_public_key: String,
    pub nonce: String,
}

impl GlobalIdentityJoinOffer {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.device_name.is_empty()
            && !self.joining_endpoint_addr.is_empty()
            && !self.joining_public_key.is_empty()
            && !self.nonce.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GlobalIdentityJoinReply {
    pub accepting_endpoint_addr: String,
    pub grant: GlobalIdentityBootstrapGrant,
    pub target_revision: String,
}

impl GlobalIdentityJoinReply {
    #[must_use]
    pub fn is_valid_for(&self, joining_public_key: &str, nonce: &str, now: i64) -> bool {
        !self.accepting_endpoint_addr.is_empty()
            && !self.target_revision.is_empty()
            && !self.target_revision.contains('/')
            && self.grant.initiating_public_key == joining_public_key
            && self.grant.nonce == nonce
            && self.grant.expires_at > now
    }
}

#[cfg(test)]
mod tests {
    use super::{GlobalIdentityJoinOffer, GlobalIdentityJoinReply};
    use crate::contract::GlobalIdentityBootstrapGrant;

    fn grant() -> GlobalIdentityBootstrapGrant {
        GlobalIdentityBootstrapGrant {
            grant_id: "grant".to_owned(),
            vault_id_hash: "vault".to_owned(),
            initiating_public_key: "joining".to_owned(),
            accepting_public_key: "accepting".to_owned(),
            nonce: "nonce".to_owned(),
            expires_at: 101,
        }
    }

    #[test]
    fn requires_a_complete_endpoint_bound_offer() {
        let offer = GlobalIdentityJoinOffer {
            device_name: "device".to_owned(),
            joining_endpoint_addr: "address".to_owned(),
            joining_public_key: "joining".to_owned(),
            nonce: "nonce".to_owned(),
        };
        assert!(offer.is_valid());
        assert!(
            !GlobalIdentityJoinOffer {
                nonce: String::new(),
                ..offer
            }
            .is_valid()
        );
    }

    #[test]
    fn binds_reply_to_the_joiner_nonce_and_revision() {
        let reply = GlobalIdentityJoinReply {
            accepting_endpoint_addr: "address".to_owned(),
            grant: grant(),
            target_revision: "revision".to_owned(),
        };
        assert!(reply.is_valid_for("joining", "nonce", 100));
        assert!(!reply.is_valid_for("other", "nonce", 100));
        assert!(!reply.is_valid_for("joining", "other", 100));
        assert!(
            !GlobalIdentityJoinReply {
                target_revision: "../revision".to_owned(),
                ..reply
            }
            .is_valid_for("joining", "nonce", 100)
        );
    }
}
