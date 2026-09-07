use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use iroh_chain::{DynamicEndpointIdStore, Server, TunnelAuthorizer, VaultId};
use lidp_model::contract::{EntityType, TunnelAuthorizationClaims};
use lidp_service::oauth2::{decode_jwt, verify_tunnel_authorization};

use lidp_server::RouterState;

pub struct LidpTunnelAuthorizer {
    state: Arc<RouterState>,
    used: std::sync::Mutex<BTreeMap<String, i64>>,
}

pub type DeviceTunnelManager = Server<DynamicEndpointIdStore, LidpTunnelAuthorizer>;

impl LidpTunnelAuthorizer {
    pub const fn new(state: Arc<RouterState>) -> Self {
        Self {
            state,
            used: std::sync::Mutex::new(BTreeMap::new()),
        }
    }
}

impl TunnelAuthorizer for LidpTunnelAuthorizer {
    async fn authorize(
        &self,
        vault_id: VaultId,
        initiating_id: iroh::EndpointId,
        accepting_id: iroh::EndpointId,
        authorization: &[u8],
    ) -> bool {
        let Ok(token) = std::str::from_utf8(authorization) else {
            return false;
        };
        let Ok((header, claims)) = decode_jwt::<TunnelAuthorizationClaims>(token) else {
            return false;
        };
        let Ok(Some(principal)) = self.state.oauth2_service.find_principal(header.kid).await else {
            return false;
        };
        if principal.get_entity_type() != EntityType::User
            || claims.sub != principal.get_entity_id().to_string()
        {
            return false;
        }
        let Ok(jwk) = self.state.oauth2_service.find_public_jwk(header.kid).await else {
            return false;
        };
        let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            return false;
        };
        if verify_tunnel_authorization(
            &jwk,
            token,
            &self.state.oauth2_service.metadata().issuer,
            &claims.sub,
            claims.application_id,
            &vault_id.hash(),
            &initiating_id.to_string(),
            &accepting_id.to_string(),
            now.as_secs() as i64,
        )
        .is_err()
        {
            return false;
        }
        consume(&self.used, token, claims.exp, now.as_secs() as i64)
    }
}

fn consume(
    used: &std::sync::Mutex<BTreeMap<String, i64>>,
    token: &str,
    expires_at: i64,
    now: i64,
) -> bool {
    let mut used = used
        .lock()
        .expect("used tunnel authorizations lock poisoned");
    used.retain(|_, expiration| *expiration > now);
    used.insert(token.to_owned(), expires_at).is_none()
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Mutex};

    use super::consume;

    #[test]
    fn consumes_each_tunnel_authorization_once() {
        let used = Mutex::new(BTreeMap::new());
        assert!(consume(&used, "grant", 10, 1));
        assert!(!consume(&used, "grant", 10, 1));
        assert!(consume(&used, "grant", 20, 10));
    }
}
