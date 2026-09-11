use std::{
    collections::BTreeMap,
    future::Future,
    io::{Error, ErrorKind},
    pin::Pin,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use iroh::EndpointId;
use iroh_chain::{TunnelAuthorizer, VaultId};
use iroh_chain_file_system::TunnelAuthorizationProvider;
use idp_model::contract::{EntityType, TunnelAuthorizationClaims, TunnelAuthorizationRequest};

use crate::{
    hosted_control_plane::HostedControlPlane,
    oauth2::{OAuth2Service, decode_jwt, verify_tunnel_authorization},
    repo::{
        DeviceRepo, LibSqlApplicationRepo, LibSqlClientRepo, LibSqlDeviceRepo, LibSqlKeyRepo,
        LibSqlOAuth2AuthorizationCodeRepo, LibSqlOAuth2UserConsentRepo, LibSqlUserRepo,
    },
    storage_session::StorageScope,
};

pub type LocalOAuth2Service = OAuth2Service<
    LibSqlApplicationRepo,
    LibSqlClientRepo,
    LibSqlOAuth2AuthorizationCodeRepo,
    LibSqlUserRepo,
    LibSqlOAuth2UserConsentRepo,
    LibSqlKeyRepo,
>;

pub struct LocalTunnelAuthorizer {
    oauth2_service: Arc<LocalOAuth2Service>,
    devices: Arc<LibSqlDeviceRepo>,
    used: Mutex<BTreeMap<String, i64>>,
}

impl LocalTunnelAuthorizer {
    #[must_use]
    pub fn new(oauth2_service: Arc<LocalOAuth2Service>, devices: Arc<LibSqlDeviceRepo>) -> Self {
        Self {
            oauth2_service,
            devices,
            used: Mutex::new(BTreeMap::new()),
        }
    }

    #[must_use]
    pub fn authorization_provider(&self, scope: StorageScope) -> LocalTunnelAuthorizationProvider {
        LocalTunnelAuthorizationProvider {
            oauth2_service: Arc::clone(&self.oauth2_service),
            devices: Arc::clone(&self.devices),
            scope,
        }
    }
}

pub struct HostedTunnelAuthorizer {
    control_plane: Arc<HostedControlPlane>,
    used: Mutex<BTreeMap<String, i64>>,
}

impl HostedTunnelAuthorizer {
    #[must_use]
    pub fn new(control_plane: Arc<HostedControlPlane>) -> Self {
        Self {
            control_plane,
            used: Mutex::new(BTreeMap::new()),
        }
    }
}

impl TunnelAuthorizer for HostedTunnelAuthorizer {
    async fn authorize(
        &self,
        vault_id: VaultId,
        initiating_id: EndpointId,
        accepting_id: EndpointId,
        authorization: &[u8],
    ) -> bool {
        let Ok(token) = std::str::from_utf8(authorization) else {
            return false;
        };
        let Ok(claims) = self
            .control_plane
            .verifies_tunnel_authorization(token, &vault_id.hash(), initiating_id, accepting_id)
            .await
        else {
            return false;
        };
        let Ok(now) = now() else {
            return false;
        };
        consume(&self.used, token, claims.exp, now)
    }
}

impl TunnelAuthorizer for LocalTunnelAuthorizer {
    async fn authorize(
        &self,
        vault_id: VaultId,
        initiating_id: EndpointId,
        accepting_id: EndpointId,
        authorization: &[u8],
    ) -> bool {
        let Ok(token) = std::str::from_utf8(authorization) else {
            return false;
        };
        let Ok((header, claims)) = decode_jwt::<TunnelAuthorizationClaims>(token) else {
            return false;
        };
        let Ok(Some(principal)) = self.oauth2_service.find_principal(header.kid).await else {
            return false;
        };
        if principal.get_entity_type() != EntityType::User
            || claims.sub != principal.get_entity_id().to_string()
        {
            return false;
        }
        let Ok(jwk) = self.oauth2_service.find_public_jwk(header.kid).await else {
            return false;
        };
        let Ok(now) = now() else {
            return false;
        };
        let initiating_public_key = initiating_id.to_string();
        let accepting_public_key = accepting_id.to_string();
        if verify_tunnel_authorization(
            &jwk,
            token,
            &self.oauth2_service.metadata().issuer,
            &claims.sub,
            claims.application_id,
            &vault_id.hash(),
            &initiating_public_key,
            &accepting_public_key,
            now,
        )
        .is_err()
        {
            return false;
        }
        let Ok(approved) = self
            .devices
            .are_approved(&initiating_public_key, &accepting_public_key)
            .await
        else {
            return false;
        };
        approved && consume(&self.used, token, claims.exp, now)
    }
}

#[derive(Clone)]
pub struct LocalTunnelAuthorizationProvider {
    oauth2_service: Arc<LocalOAuth2Service>,
    devices: Arc<LibSqlDeviceRepo>,
    scope: StorageScope,
}

#[derive(Clone)]
pub struct HostedTunnelAuthorizationProvider {
    control_plane: Arc<HostedControlPlane>,
    scope: StorageScope,
}

impl HostedTunnelAuthorizationProvider {
    #[must_use]
    pub fn new(control_plane: Arc<HostedControlPlane>, scope: StorageScope) -> Self {
        Self {
            control_plane,
            scope,
        }
    }
}

impl TunnelAuthorizationProvider for HostedTunnelAuthorizationProvider {
    fn authorization(
        &self,
        vault_id: VaultId,
        local_id: EndpointId,
        remote_id: EndpointId,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, Error>> + Send + '_>> {
        let control_plane = Arc::clone(&self.control_plane);
        let scope = self.scope.clone();
        Box::pin(async move {
            let authorization = control_plane
                .tunnel_authorization(
                    &scope.access_token,
                    TunnelAuthorizationRequest {
                        vault_id_hash: vault_id.hash(),
                        local_public_key: local_id.to_string(),
                        remote_public_key: remote_id.to_string(),
                    },
                )
                .await
                .map_err(|_| Error::new(ErrorKind::PermissionDenied, "grant was rejected"))?;
            Ok(authorization.token.into_bytes())
        })
    }
}

impl TunnelAuthorizationProvider for LocalTunnelAuthorizationProvider {
    fn authorization(
        &self,
        vault_id: VaultId,
        local_id: EndpointId,
        remote_id: EndpointId,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, Error>> + Send + '_>> {
        let oauth2_service = Arc::clone(&self.oauth2_service);
        let devices = Arc::clone(&self.devices);
        let scope = self.scope.clone();
        Box::pin(async move {
            let local_public_key = local_id.to_string();
            let remote_public_key = remote_id.to_string();
            let approved = devices
                .are_approved(&local_public_key, &remote_public_key)
                .await
                .map_err(|error| Error::other(error.to_string()))?;
            if !approved {
                return Err(Error::new(
                    ErrorKind::PermissionDenied,
                    "peer is not approved",
                ));
            }
            let principal = oauth2_service
                .find_principal(scope.principal_key_id)
                .await
                .map_err(|_| Error::other("principal lookup failed"))?
                .ok_or_else(|| Error::new(ErrorKind::PermissionDenied, "principal is missing"))?;
            let authorization = oauth2_service
                .issue_tunnel_authorization(
                    principal.as_ref(),
                    scope.application_id,
                    TunnelAuthorizationRequest {
                        vault_id_hash: vault_id.hash(),
                        local_public_key,
                        remote_public_key,
                    },
                )
                .await
                .map_err(|_| Error::new(ErrorKind::PermissionDenied, "grant was rejected"))?;
            Ok(authorization.token.into_bytes())
        })
    }
}

fn now() -> Result<i64, std::time::SystemTimeError> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64)
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
