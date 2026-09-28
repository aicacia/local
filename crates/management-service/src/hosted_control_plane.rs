use std::time::{SystemTime, UNIX_EPOCH};

use idp_service::oauth2::{decode_jwt, verify_jwt};

use idp_model::{
    contract::{
        DeviceInfo, DeviceSelfRevocationRequest, DeviceState, Jwks, StorageSession, TrustedDevice,
    },
    model::Id,
};
use model::contract::{
    AuthorizationDetail, StandardClaims, StorageAuthorizationAction, TokenType, TokenUse,
};
use reqwest::{Client, Url, redirect::Policy};
use serde::Deserialize;
use storage_model::ResourceKind;

use crate::StorageScope;

#[derive(Clone)]
pub struct HostedControlPlane {
    base_url: Url,
    expected_issuer: String,
    client: Client,
}

impl HostedControlPlane {
    pub fn new(base_url: &str) -> Result<Self, String> {
        Self::new_with_issuer(base_url, base_url.trim_end_matches('/'))
    }

    pub fn new_with_issuer(base_url: &str, expected_issuer: &str) -> Result<Self, String> {
        let issuer = Url::parse(expected_issuer)
            .map_err(|_| "issuer must be an HTTP URL without a query or fragment".to_owned())?;
        if !matches!(issuer.scheme(), "http" | "https")
            || issuer.host().is_none()
            || issuer.query().is_some()
            || issuer.fragment().is_some()
            || !issuer.username().is_empty()
            || issuer.password().is_some()
        {
            return Err("issuer must be an HTTP URL without a query or fragment".to_owned());
        }
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
            expected_issuer: expected_issuer.to_owned(),
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
            principal_key_id: idp_model::model::Id::nil(),
            trusted_devices,
            access_token: token.to_owned(),
        })
    }

    /// Returns the application ID from the authorized Storage GET only if it matches
    /// the requested application. The caller must separately authorize Management
    /// selection and device ownership before writing policy.
    pub async fn validate_storage_resource(
        &self,
        read_token: &str,
        expected_owner: &str,
        expected_storage_audience: &str,
        expected_application_id: Id,
        kind: ResourceKind,
        resource_id: &str,
    ) -> Result<Id, String> {
        let claims = self.verify_access_token(read_token).await?;
        validate_storage_read_claims(&claims, expected_owner, expected_storage_audience)?;
        self.lookup_storage_resource(read_token, expected_application_id, kind, resource_id)
            .await
    }

    async fn lookup_storage_resource(
        &self,
        read_token: &str,
        expected_application_id: Id,
        kind: ResourceKind,
        resource_id: &str,
    ) -> Result<Id, String> {
        let id: Id = resource_id
            .parse()
            .map_err(|_| "invalid storage resource ID".to_owned())?;
        if resource_id != id.to_string() {
            return Err("invalid storage resource ID".to_owned());
        }
        let collection = match kind {
            ResourceKind::Database => "databases",
            ResourceKind::FileSystem => "filesystems",
        };
        let resource: StorageResourceDetail = self
            .get(&format!("storage/{collection}/{id}"), read_token)
            .await?;
        if resource.id != resource_id {
            return Err("storage resource ID mismatch".to_owned());
        }
        if resource.application_id != expected_application_id {
            return Err("storage application ID mismatch".to_owned());
        }
        Ok(resource.application_id)
    }

    /// Selection-only check. Deselect using the locally persisted owner, without IdP access.
    pub async fn validate_selection_device(
        &self,
        read_token: &str,
        expected_actor_subject: &str,
        expected_storage_audience: &str,
        device_id: Id,
    ) -> Result<(), String> {
        let claims = self.verify_access_token(read_token).await?;
        validate_storage_read_claims(&claims, expected_actor_subject, expected_storage_audience)?;
        self.lookup_approved_device(read_token, device_id).await
    }

    async fn lookup_approved_device(&self, read_token: &str, device_id: Id) -> Result<(), String> {
        let devices: Vec<DeviceInfo> = self.get("devices", read_token).await?;
        if devices
            .iter()
            .any(|device| device.id == device_id && device.state == DeviceState::Approved)
        {
            Ok(())
        } else {
            Err("device is not an approved device of the token subject".to_owned())
        }
    }

    pub async fn trusted_devices(&self, token: &str) -> Result<Vec<TrustedDevice>, String> {
        self.get("devices/trusted", token).await
    }

    pub async fn revoke_self(&self, request: DeviceSelfRevocationRequest) -> Result<(), String> {
        self.post_empty("devices/revoke-self", &request).await
    }

    pub async fn verify_access_token(&self, token: &str) -> Result<StandardClaims, String> {
        let (header, _) =
            decode_jwt::<StandardClaims>(token).map_err(|_| "invalid token".to_owned())?;
        let jwks: Jwks = self.get(".well-known/jwks.json", "").await?;
        let jwk = jwks
            .keys
            .iter()
            .find(|jwk| jwk.kid == header.kid)
            .ok_or_else(|| "token signing key is not trusted".to_owned())?;
        let (_, claims) =
            verify_jwt::<StandardClaims>(jwk, token).map_err(|_| "invalid token".to_owned())?;
        self.check_issuer(&claims)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "system clock is before Unix epoch".to_owned())?
            .as_secs() as i64;
        if claims.r#type != TokenType::Bearer
            || claims.r#use != TokenUse::Access
            || claims.exp <= now
            || claims.iat > now
            || claims.nbf > now
            || claims.sub.is_empty()
            || claims.aud.is_empty()
            || !claims.scope.iter().any(|scope| scope == "storage")
        {
            return Err("invalid storage access token".to_owned());
        }
        Ok(claims)
    }

    fn check_issuer(&self, claims: &StandardClaims) -> Result<(), String> {
        if claims.iss != self.expected_issuer {
            return Err("token issuer is not the configured issuer".to_owned());
        }
        Ok(())
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

    async fn post_empty<B>(&self, path: &str, body: &B) -> Result<(), String>
    where
        B: serde::Serialize + ?Sized,
    {
        self.client
            .post(self.url(path)?)
            .json(body)
            .send()
            .await
            .map_err(|error| error.to_string())?
            .error_for_status()
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn url(&self, path: &str) -> Result<Url, String> {
        self.base_url.join(path).map_err(|error| error.to_string())
    }
}

#[derive(Deserialize)]
struct StorageResourceDetail {
    id: String,
    #[serde(rename = "applicationId")]
    application_id: Id,
}

fn validate_storage_read_claims(
    claims: &StandardClaims,
    expected_owner: &str,
    expected_storage_audience: &str,
) -> Result<(), String> {
    let [AuthorizationDetail::Storage(detail)] =
        claims.authorization_details.as_deref().unwrap_or(&[])
    else {
        return Err("storage read authorization required".to_owned());
    };
    if expected_owner.is_empty()
        || claims.sub != expected_owner
        || claims.client_id.is_empty()
        || expected_storage_audience.is_empty()
        || claims.aud != expected_storage_audience
        || claims.resource.as_deref() != Some(expected_storage_audience)
        || !detail.actions.contains(&StorageAuthorizationAction::Read)
    {
        return Err("storage read authorization required".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use model::contract::{
        AuthorizationDetail, StandardClaims, StorageAuthorizationAction,
        StorageAuthorizationDetail, TokenType, TokenUse,
    };
    use storage_model::ResourceKind;

    use super::{HostedControlPlane, validate_storage_read_claims};

    const ID: &str = "00000000-0000-0000-0000-000000000001";
    const APP_ID: &str = "00000000-0000-0000-0000-000000000003";

    fn claims() -> StandardClaims {
        StandardClaims {
            r#type: TokenType::Bearer,
            r#use: TokenUse::Access,
            exp: i64::MAX,
            iat: 0,
            nbf: 0,
            iss: "https://lidp.example".into(),
            aud: "storage".into(),
            client_id: "verified-client".into(),
            sub: "owner".into(),
            resource: Some("storage".into()),
            authorization_details: Some(vec![AuthorizationDetail::Storage(
                StorageAuthorizationDetail {
                    actions: vec![StorageAuthorizationAction::Read],
                },
            )]),
            scope: vec!["storage".into()],
        }
    }

    fn check(claims: &StandardClaims) -> Result<(), String> {
        validate_storage_read_claims(claims, "owner", "storage")
    }

    #[test]
    fn storage_read_requires_owner_audience_resource_and_read_detail() {
        let valid = claims();
        assert!(check(&valid).is_ok());
        assert!(validate_storage_read_claims(&valid, "other", "storage").is_err());
        assert!(validate_storage_read_claims(&valid, "owner", "other").is_err());
        assert!(validate_storage_read_claims(&valid, "", "storage").is_err());
        assert!(validate_storage_read_claims(&valid, "owner", "").is_err());
        let mut invalid = valid.clone();
        invalid.client_id.clear();
        assert!(check(&invalid).is_err());
        invalid = valid.clone();
        invalid.resource = None;
        assert!(check(&invalid).is_err());
        invalid = valid.clone();
        invalid.authorization_details = None;
        assert!(check(&invalid).is_err());
        invalid.authorization_details = Some(vec![AuthorizationDetail::Storage(
            StorageAuthorizationDetail {
                actions: vec![StorageAuthorizationAction::Write],
            },
        )]);
        assert!(check(&invalid).is_err());
        invalid.authorization_details = Some(vec![
            AuthorizationDetail::Storage(StorageAuthorizationDetail {
                actions: vec![StorageAuthorizationAction::Read],
            }),
            AuthorizationDetail::Storage(StorageAuthorizationDetail {
                actions: vec![StorageAuthorizationAction::Read],
            }),
        ]);
        assert!(check(&invalid).is_err());
    }

    async fn lookup_with_response(
        kind: ResourceKind,
        status: &str,
        body: &str,
    ) -> (Result<idp_model::model::Id, String>, String) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test HTTP server");
        let address = listener.local_addr().expect("read test HTTP address");
        let status = status.to_owned();
        let body = body.to_owned();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept test request");
            let mut request = [0; 4096];
            let length = stream.read(&mut request).expect("read test request");
            let request = String::from_utf8_lossy(&request[..length]).into_owned();
            write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).expect("write test response");
            request
        });
        let control_plane = HostedControlPlane::new(&format!("http://{address}"))
            .expect("create test control plane");
        let result = control_plane
            .lookup_storage_resource(
                "secret",
                APP_ID.parse().expect("valid application ID"),
                kind,
                ID,
            )
            .await;
        (result, server.join().expect("join test HTTP server"))
    }

    #[tokio::test]
    async fn lookup_uses_kind_specific_get_and_returns_application_id() {
        for (kind, path) in [
            (ResourceKind::Database, "databases"),
            (ResourceKind::FileSystem, "filesystems"),
        ] {
            let (result, request) = lookup_with_response(
                kind,
                "200 OK",
                &format!(r#"{{"id":"{ID}","applicationId":"{APP_ID}"}}"#),
            )
            .await;
            assert_eq!(result, Ok(APP_ID.parse().expect("valid application ID")));
            assert!(request.starts_with(&format!("GET /storage/{path}/{ID} HTTP/1.1")));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("authorization: bearer secret")
            );
        }
    }

    #[tokio::test]
    async fn lookup_rejects_wrong_or_missing_id_and_application_id() {
        for body in [
            format!(
                r#"{{"id":"00000000-0000-0000-0000-000000000002","applicationId":"{APP_ID}"}}"#
            ),
            format!(r#"{{"applicationId":"{APP_ID}"}}"#),
            format!(r#"{{"id":"{ID}","applicationId":"00000000-0000-0000-0000-000000000004"}}"#),
            format!(r#"{{"id":"{ID}"}}"#),
            format!(r#"{{"id":"{ID}","applicationId":"not-a-uuid"}}"#),
        ] {
            let (result, _) = lookup_with_response(ResourceKind::Database, "200 OK", &body).await;
            assert!(result.is_err(), "unexpectedly accepted: {body}");
        }
        let (result, _) =
            lookup_with_response(ResourceKind::FileSystem, "404 Not Found", "{}").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn lookup_rejects_invalid_id_and_unavailable_api() {
        let control_plane =
            HostedControlPlane::new("http://127.0.0.1:1").expect("create test control plane");
        assert!(
            control_plane
                .lookup_storage_resource(
                    "secret",
                    APP_ID.parse().expect("valid application ID"),
                    ResourceKind::Database,
                    "../filesystems/id"
                )
                .await
                .is_err()
        );
        assert!(
            control_plane
                .lookup_storage_resource(
                    "secret",
                    APP_ID.parse().expect("valid application ID"),
                    ResourceKind::Database,
                    ID
                )
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn selection_device_lookup_requires_approved_owner_scoped_device() {
        for (body, allowed) in [
            (format!(r#"[{{"id":"{ID}","name":"test","publicKey":"key","address":"addr","state":"approved","createdAt":0,"updatedAt":0,"revokedAt":null}}]"#), true),
            (format!(r#"[{{"id":"{ID}","name":"test","publicKey":"key","address":"addr","state":"pending","createdAt":0,"updatedAt":0,"revokedAt":null}}]"#), false),
            (format!(r#"[{{"id":"{ID}","name":"test","publicKey":"key","address":"addr","state":"revoked","createdAt":0,"updatedAt":0,"revokedAt":0}}]"#), false),
            (r#"[]"#.to_owned(), false),
            (r#"[{"id":"00000000-0000-0000-0000-000000000002","name":"other","publicKey":"key","address":"addr","state":"approved","createdAt":0,"updatedAt":0,"revokedAt":null}]"#.to_owned(), false),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test HTTP server");
            let address = listener.local_addr().expect("read test HTTP address");
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept test request");
                let mut request = [0; 4096];
                let length = stream.read(&mut request).expect("read test request");
                let request = String::from_utf8_lossy(&request[..length]).into_owned();
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).expect("write test response");
                request
            });
            let control_plane = HostedControlPlane::new(&format!("http://{address}"))
                .expect("create test control plane");
            let result = control_plane.lookup_approved_device("secret", ID.parse().expect("valid device ID")).await;
            assert_eq!(result.is_ok(), allowed);
            let request = server.join().expect("join test HTTP server");
            assert!(request.starts_with("GET /devices HTTP/1.1"));
            assert!(request.to_ascii_lowercase().contains("authorization: bearer secret"));
        }
    }

    #[tokio::test]
    async fn selection_device_lookup_rejects_idp_outage() {
        let control_plane =
            HostedControlPlane::new("http://127.0.0.1:1").expect("create test control plane");
        assert!(
            control_plane
                .lookup_approved_device("secret", ID.parse().expect("valid device ID"))
                .await
                .is_err()
        );
    }

    #[test]
    fn issuer_is_independent_of_api_base() {
        let issuer = idp_service::oauth2::OAuth2Config::default().issuer;
        let control_plane = HostedControlPlane::new_with_issuer("https://api.example", &issuer)
            .expect("create control plane with distinct issuer");
        let mut signed_claims = claims();
        signed_claims.iss = issuer.to_owned();
        assert!(control_plane.check_issuer(&signed_claims).is_ok());
        signed_claims.iss = "https://api.example".into();
        assert!(control_plane.check_issuer(&signed_claims).is_err());
        let other_api = HostedControlPlane::new_with_issuer("https://other-api.example", &issuer)
            .expect("create control plane with different API base");
        assert!(other_api.check_issuer(&signed_claims).is_err());
        signed_claims.iss = issuer.to_owned();
        assert!(other_api.check_issuer(&signed_claims).is_ok());
    }

    #[test]
    fn rejects_invalid_issuers() {
        for issuer in [
            "",
            "not-a-url",
            "ftp://idp.example",
            "https://",
            "https://idp.example/?x=1",
            "https://idp.example/#fragment",
            "https://user@idp.example",
        ] {
            assert!(
                HostedControlPlane::new_with_issuer("https://api.example", issuer).is_err(),
                "accepted issuer {issuer}"
            );
        }
        assert!(
            HostedControlPlane::new_with_issuer("ftp://api.example", "https://idp.example")
                .is_err()
        );
    }

    #[test]
    fn accepts_only_http_control_plane_urls() {
        let control_plane = HostedControlPlane::new("https://lidp.example/")
            .expect("create control plane using API URL as issuer");
        let mut signed_claims = claims();
        signed_claims.iss = "https://lidp.example".into();
        assert!(control_plane.check_issuer(&signed_claims).is_ok());
        assert!(HostedControlPlane::new("https://lidp.example").is_ok());
        assert!(HostedControlPlane::new("ftp://lidp.example").is_err());
        assert!(HostedControlPlane::new("https://lidp.example/?x=1").is_err());
    }
}
