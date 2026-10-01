use std::{collections::BTreeMap, sync::Mutex, time::Duration};

use idp_model::model::Id;

use iroh::EndpointId;
use iroh_chain::Server;

const GRANT_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Clone)]
pub struct BootstrapGrant {
    pub endpoint_id: EndpointId,
    pub device_id: Id,
    pub enrollment_code_hash: Vec<u8>,
    expires_at: std::time::Instant,
    in_use: bool,
}

impl BootstrapGrant {
    pub fn expires_at_unix(&self) -> i64 {
        std::time::SystemTime::now()
            .checked_add(
                self.expires_at
                    .saturating_duration_since(std::time::Instant::now()),
            )
            .and_then(|expiry| expiry.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |expiry| expiry.as_secs() as i64)
    }
}

#[derive(Default)]
pub struct BootstrapRegistry {
    grants: Mutex<BTreeMap<String, BootstrapGrant>>,
    admission: Mutex<Option<(Server, Vec<u8>)>>,
}

impl BootstrapRegistry {
    pub fn set_admission_server(&self, server: Server, alpn: &[u8]) {
        *self
            .admission
            .lock()
            .expect("bootstrap admission lock poisoned") = Some((server, alpn.to_vec()));
    }

    pub fn register(
        &self,
        grant_id: String,
        endpoint_id: EndpointId,
        device_id: Id,
        enrollment_code_hash: Vec<u8>,
    ) -> (String, BootstrapGrant) {
        let grant = BootstrapGrant {
            endpoint_id,
            device_id,
            enrollment_code_hash,
            expires_at: std::time::Instant::now() + GRANT_TTL,
            in_use: false,
        };
        let mut grants = self.grants.lock().expect("bootstrap grant lock poisoned");
        grants.retain(|_, grant| grant.expires_at > std::time::Instant::now());
        grants.insert(grant_id.clone(), grant.clone());
        drop(grants);
        if let Some((server, alpn)) = self
            .admission
            .lock()
            .expect("bootstrap admission lock poisoned")
            .as_ref()
        {
            server.allow_peer_for_alpn(endpoint_id, alpn, GRANT_TTL);
        }
        (grant_id, grant)
    }

    pub fn get(&self, grant_id: &str) -> Option<BootstrapGrant> {
        self.grants
            .lock()
            .expect("bootstrap grant lock poisoned")
            .get(grant_id)
            .cloned()
    }

    pub fn reserve(&self, grant_id: &str, endpoint_id: EndpointId) -> bool {
        let mut grants = self.grants.lock().expect("bootstrap grant lock poisoned");
        grants.retain(|_, grant| grant.expires_at > std::time::Instant::now());
        let Some(grant) = grants.get_mut(grant_id) else {
            return false;
        };
        if grant.endpoint_id != endpoint_id || grant.in_use {
            return false;
        }
        grant.in_use = true;
        true
    }

    pub fn finish(&self, grant_id: &str, endpoint_id: EndpointId, success: bool) {
        let mut grants = self.grants.lock().expect("bootstrap grant lock poisoned");
        if success {
            if grants
                .get(grant_id)
                .is_some_and(|grant| grant.endpoint_id == endpoint_id)
            {
                grants.remove(grant_id);
            }
        } else if let Some(grant) = grants.get_mut(grant_id)
            && grant.endpoint_id == endpoint_id
        {
            grant.in_use = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use idp_model::model::Id;
    use iroh::SecretKey;

    use super::BootstrapRegistry;

    #[test]
    fn binds_grant_to_peer_and_allows_retry_after_failure() {
        let registry = BootstrapRegistry::default();
        let peer = SecretKey::generate().public();
        let other = SecretKey::generate().public();
        let (grant_id, _) = registry.register("grant".into(), peer, Id::now_v7(), vec![]);

        assert!(!registry.reserve(&grant_id, other));
        assert!(!registry.reserve("wrong-token", peer));
        assert!(registry.reserve(&grant_id, peer));
        assert!(!registry.reserve(&grant_id, peer));
        registry.finish(&grant_id, peer, false);
        assert!(registry.reserve(&grant_id, peer));
        registry.finish(&grant_id, peer, true);
        assert!(!registry.reserve(&grant_id, peer));
    }
}
