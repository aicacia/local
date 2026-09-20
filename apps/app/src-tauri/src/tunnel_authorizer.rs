use std::sync::Arc;

use iroh_chain::{DynamicEndpointIdStore, Server, TunnelAuthorizer, VaultId};
use management_service::{
    HostedControlPlane, access_token_authorization::HostedAccessTokenAuthorizer,
};

pub struct AppTunnelAuthorizer {
    hosted: Option<HostedAccessTokenAuthorizer>,
}

pub type DeviceTunnelManager = Server<DynamicEndpointIdStore, AppTunnelAuthorizer>;

impl AppTunnelAuthorizer {
    pub fn new(control_plane: Option<Arc<HostedControlPlane>>) -> Self {
        Self {
            hosted: control_plane.map(HostedAccessTokenAuthorizer::new),
        }
    }
}

impl TunnelAuthorizer for AppTunnelAuthorizer {
    async fn authorize(
        &self,
        vault_id: VaultId,
        initiating_id: iroh::EndpointId,
        accepting_id: iroh::EndpointId,
        authorization: &[u8],
    ) -> bool {
        let Some(authorizer) = &self.hosted else {
            return false;
        };
        authorizer
            .authorize(vault_id, initiating_id, accepting_id, authorization)
            .await
    }
}
