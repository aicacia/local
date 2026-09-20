use std::{future::Future, io::Error, sync::Arc};

use crate::{HostedControlPlane, StorageScope};
use iroh::EndpointId;
use iroh_chain::{TunnelAuthorizer, VaultId};
use iroh_chain_file_system::AccessTokenProvider;

pub struct HostedAccessTokenAuthorizer {
    control_plane: Arc<HostedControlPlane>,
}

impl HostedAccessTokenAuthorizer {
    #[must_use]
    pub fn new(control_plane: Arc<HostedControlPlane>) -> Self {
        Self { control_plane }
    }
}

impl TunnelAuthorizer for HostedAccessTokenAuthorizer {
    async fn authorize(
        &self,
        _: VaultId,
        initiating_id: EndpointId,
        accepting_id: EndpointId,
        authorization: &[u8],
    ) -> bool {
        let Ok(token) = std::str::from_utf8(authorization) else {
            return false;
        };
        if self.control_plane.verify_access_token(token).await.is_err() {
            return false;
        }
        let Ok(devices) = self.control_plane.trusted_devices(token).await else {
            return false;
        };
        let initiating_id = initiating_id.to_string();
        let accepting_id = accepting_id.to_string();
        devices
            .iter()
            .any(|device| device.public_key == initiating_id)
            && devices
                .iter()
                .any(|device| device.public_key == accepting_id)
    }
}

#[derive(Clone)]
pub struct HostedAccessTokenProvider {
    scope: StorageScope,
}

impl HostedAccessTokenProvider {
    #[must_use]
    pub fn new(scope: StorageScope) -> Self {
        Self { scope }
    }
}

impl AccessTokenProvider for HostedAccessTokenProvider {
    fn access_token(
        &self,
        _: VaultId,
        _: EndpointId,
        _: EndpointId,
    ) -> impl Future<Output = Result<Vec<u8>, Error>> + Send {
        let token = self.scope.access_token.clone();
        async move { Ok(token.into_bytes()) }
    }
}
