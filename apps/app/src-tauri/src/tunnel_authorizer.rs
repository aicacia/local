use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use idp_service::{
    libsql::{
        LibSqlApplicationRepo, LibSqlClientRepo, LibSqlKeyRepo, LibSqlOAuth2AuthorizationCodeRepo,
        LibSqlOAuth2UserConsentRepo, LibSqlUserRepo,
    },
    oauth2::OAuth2Service,
};
use iroh_chain::{DynamicEndpointIdStore, Server, TunnelAuthorizer, VaultId};
use management_service::{
    libsql::LibSqlDeviceRepo,
    tunnel_authorization::{LocalTunnelAuthorizationProvider, LocalTunnelAuthorizer},
};

use management_service::HostedControlPlane;

type LocalOAuth2Service = OAuth2Service<
    LibSqlApplicationRepo,
    LibSqlClientRepo,
    LibSqlOAuth2AuthorizationCodeRepo,
    LibSqlUserRepo,
    LibSqlOAuth2UserConsentRepo,
    LibSqlKeyRepo,
>;
pub type LocalTunnel = LocalTunnelAuthorizer<LocalOAuth2Service, LibSqlDeviceRepo>;
pub type LocalTunnelProvider =
    LocalTunnelAuthorizationProvider<LocalOAuth2Service, LibSqlDeviceRepo>;

pub struct LidpTunnelAuthorizer {
    local: Arc<LocalTunnel>,
    control_plane: Option<Arc<HostedControlPlane>>,
    used: Mutex<BTreeMap<String, i64>>,
}

pub type DeviceTunnelManager = Server<DynamicEndpointIdStore, LidpTunnelAuthorizer>;

impl LidpTunnelAuthorizer {
    pub fn new(
        state: Arc<idp_server::RouterState>,
        control_plane: Option<Arc<HostedControlPlane>>,
    ) -> Self {
        Self {
            local: Arc::new(LocalTunnelAuthorizer::new(
                Arc::clone(&state.oauth2_service),
                Arc::clone(&state.devices),
            )),
            control_plane,
            used: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn local(&self) -> Arc<LocalTunnel> {
        Arc::clone(&self.local)
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
        let Some(control_plane) = &self.control_plane else {
            return self
                .local
                .authorize(vault_id, initiating_id, accepting_id, authorization)
                .await;
        };
        let Ok(token) = std::str::from_utf8(authorization) else {
            return false;
        };
        let Ok(claims) = control_plane
            .verifies_tunnel_authorization(token, &vault_id.hash(), initiating_id, accepting_id)
            .await
        else {
            return false;
        };
        let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            return false;
        };
        consume(&self.used, token, claims.exp, now.as_secs() as i64)
    }
}

fn consume(used: &Mutex<BTreeMap<String, i64>>, token: &str, expires_at: i64, now: i64) -> bool {
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
