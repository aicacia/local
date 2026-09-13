use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use idp_model::contract::GlobalIdentityBootstrapGrant;
use idp_service::generate_random_string;

use iroh::EndpointId;
use iroh_chain::{TunnelAuthorizer, VaultId};

pub struct GlobalBootstrapGrants {
    grants: Mutex<BTreeMap<String, GlobalIdentityBootstrapGrant>>,
}

pub struct GlobalBootstrapTunnelAuthorizer {
    grants: Arc<GlobalBootstrapGrants>,
}

impl GlobalBootstrapGrants {
    #[must_use]
    pub fn new() -> Self {
        Self {
            grants: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn issue(
        &self,
        initiating_public_key: String,
        accepting_public_key: String,
        nonce: String,
        expires_at: i64,
    ) -> GlobalIdentityBootstrapGrant {
        let grant = GlobalIdentityBootstrapGrant {
            grant_id: generate_random_string::<32>(),
            vault_id_hash: VaultId::global_identity().hash(),
            initiating_public_key,
            accepting_public_key,
            nonce,
            expires_at,
        };
        self.grants
            .lock()
            .expect("global bootstrap grants lock poisoned")
            .insert(grant.grant_id.clone(), grant.clone());
        grant
    }

    pub fn consume(
        &self,
        grant_id: &str,
        initiating_public_key: &str,
        accepting_public_key: &str,
        nonce: &str,
        now: i64,
    ) -> Option<GlobalIdentityBootstrapGrant> {
        let mut grants = self
            .grants
            .lock()
            .expect("global bootstrap grants lock poisoned");
        grants.retain(|_, grant| grant.expires_at > now);
        let grant = grants.get(grant_id)?;
        if !grant.valid_for(
            &VaultId::global_identity().hash(),
            initiating_public_key,
            accepting_public_key,
            nonce,
            now,
        ) {
            return None;
        }
        grants.remove(grant_id)
    }
}

impl Default for GlobalBootstrapGrants {
    fn default() -> Self {
        Self::new()
    }
}

impl GlobalBootstrapTunnelAuthorizer {
    #[must_use]
    pub fn new(grants: Arc<GlobalBootstrapGrants>) -> Self {
        Self { grants }
    }

    #[must_use]
    pub fn grants(&self) -> &Arc<GlobalBootstrapGrants> {
        &self.grants
    }
}

impl TunnelAuthorizer for GlobalBootstrapTunnelAuthorizer {
    async fn authorize(
        &self,
        vault_id: VaultId,
        initiating_id: EndpointId,
        accepting_id: EndpointId,
        authorization: &[u8],
    ) -> bool {
        if vault_id != VaultId::global_identity() {
            return false;
        }
        let Ok(grant) = serde_json::from_slice::<GlobalIdentityBootstrapGrant>(authorization)
        else {
            return false;
        };
        let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            return false;
        };
        self.grants
            .consume(
                &grant.grant_id,
                &initiating_id.to_string(),
                &accepting_id.to_string(),
                &grant.nonce,
                i64::try_from(now.as_secs()).unwrap_or(i64::MAX),
            )
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use iroh::SecretKey;
    use iroh_chain::{TunnelAuthorizer, VaultId};

    use std::sync::Arc;

    use super::{GlobalBootstrapGrants, GlobalBootstrapTunnelAuthorizer};

    #[test]
    fn grants_are_endpoint_bound_expiring_and_single_use() {
        let grants = GlobalBootstrapGrants::new();
        let grant = grants.issue(
            "joining".to_owned(),
            "approved".to_owned(),
            "nonce".to_owned(),
            101,
        );

        assert!(
            grants
                .consume(&grant.grant_id, "joining", "approved", &grant.nonce, 100)
                .is_some()
        );
        assert!(
            grants
                .consume(&grant.grant_id, "joining", "approved", &grant.nonce, 100)
                .is_none()
        );

        let expired = grants.issue(
            "joining".to_owned(),
            "approved".to_owned(),
            "expired-nonce".to_owned(),
            100,
        );
        assert!(
            grants
                .consume(
                    &expired.grant_id,
                    "joining",
                    "approved",
                    &expired.nonce,
                    100,
                )
                .is_none()
        );
    }

    #[tokio::test]
    async fn tunnel_authorizer_consumes_only_global_bootstrap_grants() {
        let joining = SecretKey::generate().public();
        let accepting = SecretKey::generate().public();
        let grants = GlobalBootstrapGrants::new();
        let grant = grants.issue(
            joining.to_string(),
            accepting.to_string(),
            "nonce".to_owned(),
            i64::MAX,
        );
        let authorization = serde_json::to_vec(&grant).expect("serializes grant");
        let authorizer = GlobalBootstrapTunnelAuthorizer::new(Arc::new(grants));

        assert!(
            authorizer
                .authorize(
                    VaultId::global_identity(),
                    joining,
                    accepting,
                    &authorization
                )
                .await
        );
        assert!(
            !authorizer
                .authorize(
                    VaultId::global_identity(),
                    joining,
                    accepting,
                    &authorization
                )
                .await
        );
    }
}
