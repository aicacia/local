use std::time::{SystemTime, UNIX_EPOCH};

use idp_service::oauth2::{decode_jwt, verify_jwt, verify_tunnel_authorization};

use crate::StorageScope;
use idp_model::contract::{
    Jwks, StorageSession, TrustedDevice, TunnelAuthorization, TunnelAuthorizationClaims,
    TunnelAuthorizationRequest,
};
use iroh::EndpointId;
use model::contract::{StandardClaims, TokenType, TokenUse};
use reqwest::{Client, Url, redirect::Policy};

#[derive(Clone)]
pub struct HostedControlPlane {
    base_url: Url,
    client: Client,
}

impl HostedControlPlane {
    pub fn new(base_url: &str) -> Result<Self, String> {
        let mut base_url = Url::parse(base_url).map_err(|error| error.to_string())?;
        if !matches!(base_url.scheme(), "http" | "https") || base_url.query().is_some() {
            return Err("control plane URI must be an HTTP URL without a query".to_owned());
        }
        base_url.set_fragment(None);
        if !base_url.path().ends_with('/') {
            base_url.set_path(&format!("{}/", base_url.path()));
        }
        Ok(Self {
            base_url,
            client: Client::builder()
                .redirect(Policy::none())
                .build()
                .map_err(|error| error.to_string())?,
        })
    }

    pub async fn storage_scope(&self, token: &str) -> Result<StorageScope, String> {
        let claims = self.verify_access_token(token).await?;
        let session: StorageSession = self.post("storage/sessions", token, &()).await?;
        let trusted_devices = self.trusted_devices(token).await?;
        Ok(StorageScope {
            user_sub: claims.sub,
            application_id: session.application_id,
            principal_key_id: 0,
            trusted_devices,
            access_token: token.to_owned(),
        })
    }

    pub async fn trusted_devices(&self, token: &str) -> Result<Vec<TrustedDevice>, String> {
        self.get("devices/trusted", token).await
    }

    pub async fn tunnel_authorization(
        &self,
        token: &str,
        request: TunnelAuthorizationRequest,
    ) -> Result<TunnelAuthorization, String> {
        self.post("devices/tunnels", token, &request).await
    }

    pub async fn verifies_tunnel_authorization(
        &self,
        token: &str,
        vault_id_hash: &str,
        initiating_id: EndpointId,
        accepting_id: EndpointId,
    ) -> Result<TunnelAuthorizationClaims, String> {
        let (header, claims) = decode_jwt::<TunnelAuthorizationClaims>(token)
            .map_err(|_| "invalid grant".to_owned())?;
        if claims.iss != self.base_url.as_str().trim_end_matches('/') {
            return Err("grant issuer is not the configured control plane".to_owned());
        }
        let jwks: Jwks = self.get(".well-known/jwks.json", "").await?;
        let jwk = jwks
            .keys
            .iter()
            .find(|jwk| jwk.kid == header.kid)
            .ok_or_else(|| "grant signing key is not trusted".to_owned())?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "system clock is before Unix epoch".to_owned())?
            .as_secs() as i64;
        verify_tunnel_authorization(
            jwk,
            token,
            self.base_url.as_str().trim_end_matches('/'),
            &claims.sub,
            claims.application_id,
            vault_id_hash,
            &initiating_id.to_string(),
            &accepting_id.to_string(),
            now,
        )
        .map_err(|_| "invalid grant".to_owned())?;
        Ok(claims)
    }

    async fn verify_access_token(&self, token: &str) -> Result<StandardClaims, String> {
        let (header, claims) =
            decode_jwt::<StandardClaims>(token).map_err(|_| "invalid token".to_owned())?;
        if claims.iss != self.base_url.as_str().trim_end_matches('/') {
            return Err("token issuer is not the configured control plane".to_owned());
        }
        let jwks: Jwks = self.get(".well-known/jwks.json", "").await?;
        let jwk = jwks
            .keys
            .iter()
            .find(|jwk| jwk.kid == header.kid)
            .ok_or_else(|| "token signing key is not trusted".to_owned())?;
        let (_, claims) =
            verify_jwt::<StandardClaims>(jwk, token).map_err(|_| "invalid token".to_owned())?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "system clock is before Unix epoch".to_owned())?
            .as_secs() as i64;
        if claims.r#type != TokenType::Bearer
            || claims.r#use != TokenUse::Access
            || claims.exp <= now
            || claims.nbf > now
            || claims.sub.is_empty()
            || claims.aud.is_empty()
            || !claims.scope.iter().any(|scope| scope == "storage")
        {
            return Err("invalid storage access token".to_owned());
        }
        Ok(claims)
    }

    async fn get<T>(&self, path: &str, token: &str) -> Result<T, String>
    where
        T: serde::de::DeserializeOwned,
    {
        let mut request = self.client.get(self.url(path)?);
        if !token.is_empty() {
            request = request.bearer_auth(token);
        }
        request
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .json()
            .await
            .map_err(|error| error.to_string())
    }

    async fn post<T, B>(&self, path: &str, token: &str, body: &B) -> Result<T, String>
    where
        T: serde::de::DeserializeOwned,
        B: serde::Serialize + ?Sized,
    {
        self.client
            .post(self.url(path)?)
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?
            .json()
            .await
            .map_err(|error| error.to_string())
    }

    fn url(&self, path: &str) -> Result<Url, String> {
        self.base_url.join(path).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::HostedControlPlane;

    #[test]
    fn accepts_only_http_control_plane_urls() {
        assert!(HostedControlPlane::new("https://lidp.example").is_ok());
        assert!(HostedControlPlane::new("ftp://lidp.example").is_err());
        assert!(HostedControlPlane::new("https://lidp.example/?x=1").is_err());
    }
}
