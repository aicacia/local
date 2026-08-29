use std::{
    fs, io,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use async_trait::async_trait;
use axum::{
    Router,
    extract::ws::{Message, WebSocket},
    extract::{State, WebSocketUpgrade},
    response::{Html, IntoResponse},
    routing::get,
};
use lidp_model::contract::JwkPublic;
use lidp_service::oauth2::{decode_jwt, verify_jwt};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair, SanType,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Serialize};
use serde_json::json;
use storage_iroh::{StorageEvent, StorageIrohRuntime, StorageRequest, StorageResponse};
use storage_service::StorageService;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use crate::bridge_trust::install_ca_to_user_trust_store;

pub const STORAGE_BRIDGE_HOST: &str = "storage.localhost";

const TRUST_PAGE: &str = "<!DOCTYPE html>\
<html lang=\"en\">\
<head>\
<meta charset=\"utf-8\">\
<title>Storage Bridge Certificate</title>\
</head>\
<body>\
<h1>Storage bridge certificate</h1>\
<p>If your browser shows a certificate warning, accept it to allow local web apps to connect.</p>\
<p>You can close this tab after the page loads without warnings.</p>\
</body>\
</html>";

pub fn bridge_cert_path(data_dir: &Path) -> PathBuf {
    data_dir.join("storage-bridge-ca.pem")
}

fn ca_key_path(data_dir: &Path) -> PathBuf {
    data_dir.join("storage-bridge-ca.key")
}

pub fn bridge_url(host: &str, port: u16) -> String {
    format!("wss://{host}:{port}")
}

pub fn bridge_trust_url(wss_url: &str) -> Option<String> {
    wss_url
        .strip_prefix("wss://")
        .map(|rest| format!("https://{rest}/trust"))
}

fn host_file_suffix(host: &str) -> String {
    host.replace('.', "-")
}

fn server_cert_paths(data_dir: &Path, host: &str) -> (PathBuf, PathBuf, PathBuf) {
    let suffix = host_file_suffix(host);
    (
        data_dir.join(format!("storage-bridge-server-{suffix}.der")),
        data_dir.join(format!("storage-bridge-server-{suffix}.key")),
        data_dir.join(format!("storage-bridge-server-{suffix}.pem")),
    )
}

pub async fn ensure_storage_bridge_certificate(data_dir: &Path) -> io::Result<PathBuf> {
    let cert_path = bridge_cert_path(data_dir);
    let (ca, is_new_ca) = load_or_create_ca(data_dir).await?;
    let (_ca_pem, is_new_pem) = ensure_ca_certificate_pem(data_dir, &ca).await?;
    let _server_cert = load_or_create_server_cert(STORAGE_BRIDGE_HOST, data_dir, &ca).await?;

    if is_new_ca || is_new_pem {
        let _ = install_ca_to_user_trust_store(&cert_path);
    }

    Ok(cert_path)
}

#[derive(Clone)]
struct CertificateFiles {
    cert_der: Vec<u8>,
    key_der: Vec<u8>,
    cert_pem: String,
}

async fn load_or_create_ca(data_dir: &Path) -> io::Result<(KeyPair, bool)> {
    let key_path = ca_key_path(data_dir);

    if fs::exists(&key_path)? {
        log::debug!("Loading existing CA key from {:?}", key_path);
        let key = fs::read_to_string(&key_path)?;
        return Ok((KeyPair::from_pem(&key).map_err(io::Error::other)?, false));
    }

    let key = KeyPair::generate().map_err(io::Error::other)?;
    log::debug!("Generated new CA key and saving to {:?}", key_path);
    fs::write(&key_path, key.serialize_pem())?;

    Ok((key, true))
}

async fn ensure_ca_certificate_pem(
    data_dir: &Path,
    ca_key: &KeyPair,
) -> io::Result<(String, bool)> {
    let ca_pem_path = bridge_cert_path(data_dir);
    let ca_der_path = ca_der_path(data_dir);

    if fs::exists(&ca_pem_path)? && fs::exists(&ca_der_path)? {
        return Ok((fs::read_to_string(&ca_pem_path)?, false));
    }

    let ca_cert = generate_ca_certificate(ca_key)?;
    let ca_pem = ca_cert.pem();
    let ca_der = ca_cert.der().to_vec();
    fs::write(&ca_pem_path, ca_pem.as_bytes())?;
    fs::write(&ca_der_path, &ca_der)?;
    Ok((ca_pem, true))
}

fn ca_der_path(data_dir: &Path) -> PathBuf {
    data_dir.join("storage-bridge-ca.der")
}

fn generate_ca_certificate(ca_key: &KeyPair) -> io::Result<rcgen::Certificate> {
    let mut ca_params = CertificateParams::new(vec![]).map_err(io::Error::other)?;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.distinguished_name = DistinguishedName::new();
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "Local Storage Bridge CA");

    ca_params.self_signed(ca_key).map_err(io::Error::other)
}

async fn load_or_create_server_cert(
    host: &str,
    data_dir: &Path,
    ca_key: &KeyPair,
) -> io::Result<CertificateFiles> {
    let (cert_path, key_path, cert_pem_path) = server_cert_paths(data_dir, host);

    if fs::exists(&cert_path)? && fs::exists(&key_path)? && fs::exists(&cert_pem_path)? {
        log::debug!(
            "Loading existing server certificate and key from {:?} and {:?}",
            cert_path,
            key_path
        );
        return Ok(CertificateFiles {
            cert_der: fs::read(&cert_path)?,
            key_der: fs::read(&key_path)?,
            cert_pem: fs::read_to_string(&cert_pem_path)?,
        });
    }

    let mut params = CertificateParams::new(vec![host.to_string(), "localhost".to_string()])
        .map_err(io::Error::other)?;
    params
        .subject_alt_names
        .push(SanType::IpAddress(Ipv4Addr::LOCALHOST.into()));
    params.distinguished_name = DistinguishedName::new();
    params.distinguished_name.push(DnType::CommonName, host);
    params.is_ca = IsCa::NoCa;

    let key = KeyPair::generate().map_err(io::Error::other)?;

    let ca_params = {
        let mut params = CertificateParams::new(vec![]).map_err(io::Error::other)?;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, "Local Storage Bridge CA");
        params
    };

    let ca_issuer = Issuer::new(ca_params, ca_key);
    let cert = params
        .signed_by(&key, &ca_issuer)
        .map_err(io::Error::other)?;

    let cert_der = cert.der().to_vec();
    let key_der = key.serialize_der();
    let cert_pem = cert.pem();

    log::debug!(
        "Generated new server certificate and key, saving to {:?} and {:?}",
        cert_path,
        key_path
    );
    fs::write(&cert_path, &cert_der)?;
    fs::write(&key_path, &key_der)?;
    fs::write(cert_pem_path, cert_pem.as_bytes())?;

    Ok(CertificateFiles {
        cert_der,
        key_der,
        cert_pem,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
enum BridgeMessage {
    Request {
        #[serde(flatten)]
        request: StorageRequest,
        #[serde(rename = "requestId")]
        request_id: u64,
        #[serde(default)]
        authorization: Option<String>,
    },
    Response {
        #[serde(flatten)]
        response: StorageResponse,
        #[serde(rename = "requestId")]
        request_id: u64,
    },
    Event(StorageEvent),
}

struct TlsListener {
    inner: TcpListener,
    acceptor: TlsAcceptor,
}

impl axum::serve::Listener for TlsListener {
    type Io = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;
    type Addr = std::net::SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.inner.accept().await {
                Ok((stream, addr)) => match self.acceptor.clone().accept(stream).await {
                    Ok(tls_stream) => return (tls_stream, addr),
                    Err(err) => {
                        log::warn!("storage bridge TLS accept failed: {err}");
                        continue;
                    }
                },
                Err(err) => {
                    log::warn!("storage bridge TCP accept failed: {err}");
                    continue;
                }
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}

#[derive(Debug, Clone)]
pub struct StorageBridge {
    runtime: StorageIrohRuntime,
    storage: Arc<StorageService>,
    url: Arc<tokio::sync::Mutex<String>>,
    registry: Arc<libsql::Database>,
    jwt_verifier: Option<Arc<StorageJwtVerifier>>,
}

#[derive(Debug, Clone, Deserialize)]
struct StorageSessionClaims {
    sub: String,
    iss: String,
    aud: String,
    exp: i64,
    nbf: i64,
}

#[async_trait]
pub trait PrincipalKeyProvider: Send + Sync {
    async fn public_key(
        &self,
        issuer: &str,
        key_id: u32,
        candidate: &JwkPublic,
    ) -> Result<JwkPublic, String>;
}

#[derive(Debug, Clone)]
pub struct SqlitePrincipalKeyProvider {
    database: Arc<libsql::Database>,
}

impl SqlitePrincipalKeyProvider {
    pub async fn new(database: Arc<libsql::Database>) -> Result<Self, String> {
        storage_model::migrate::up(&database)
            .await
            .map_err(|error| error.to_string())?;
        Ok(Self { database })
    }

    pub async fn put_issuer(&self, issuer: &str) -> Result<(), String> {
        let connection = self.database.connect().map_err(|error| error.to_string())?;
        connection
            .execute(
                "INSERT INTO storage_issuers (issuer, revoked_at) VALUES (?, NULL) ON CONFLICT(issuer) DO UPDATE SET revoked_at = NULL",
                libsql::params![issuer],
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub async fn put_key(&self, issuer: &str, key: &JwkPublic) -> Result<(), String> {
        let public_key = serde_json::to_string(&key.params).map_err(|error| error.to_string())?;
        self.put_issuer(issuer).await?;
        let connection = self.database.connect().map_err(|error| error.to_string())?;
        connection
            .execute(
                "INSERT INTO storage_issuer_keys (issuer, key_id, public_key, revoked_at) VALUES (?, ?, ?, NULL) ON CONFLICT(issuer, key_id) DO UPDATE SET public_key = excluded.public_key, revoked_at = NULL",
                libsql::params![issuer, key.kid as i64, public_key],
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub async fn revoke_issuer(&self, issuer: &str) -> Result<(), String> {
        let connection = self.database.connect().map_err(|error| error.to_string())?;
        connection
            .execute(
                "UPDATE storage_issuers SET revoked_at = unixepoch() WHERE issuer = ?",
                libsql::params![issuer],
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub async fn revoke_key(&self, issuer: &str, key_id: u32) -> Result<(), String> {
        let connection = self.database.connect().map_err(|error| error.to_string())?;
        connection
            .execute(
                "UPDATE storage_issuer_keys SET revoked_at = unixepoch() WHERE issuer = ? AND key_id = ?",
                libsql::params![issuer, key_id as i64],
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[async_trait]
impl PrincipalKeyProvider for SqlitePrincipalKeyProvider {
    async fn public_key(
        &self,
        issuer: &str,
        key_id: u32,
        candidate: &JwkPublic,
    ) -> Result<JwkPublic, String> {
        let connection = self.database.connect().map_err(|error| error.to_string())?;
        let mut rows = connection
            .query(
                "SELECT k.public_key FROM storage_issuers i JOIN storage_issuer_keys k ON k.issuer = i.issuer WHERE i.issuer = ? AND i.revoked_at IS NULL AND k.key_id = ? AND k.revoked_at IS NULL LIMIT 1",
                libsql::params![issuer, key_id as i64],
            )
            .await
            .map_err(|error| error.to_string())?;
        let Some(row) = rows.next().await.map_err(|error| error.to_string())? else {
            return Err("JWT signing key is unknown or revoked".to_string());
        };
        let public_key = row.get::<String>(0).map_err(|error| error.to_string())?;
        let candidate_key =
            serde_json::to_string(&candidate.params).map_err(|error| error.to_string())?;
        if public_key != candidate_key {
            return Err("JWT public key is not trusted for this issuer".to_string());
        }
        Ok(candidate.clone())
    }
}

#[derive(Clone)]
pub struct StorageJwtVerifier {
    key_provider: Arc<dyn PrincipalKeyProvider>,
    issuer: String,
    audience: String,
}

impl std::fmt::Debug for StorageJwtVerifier {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StorageJwtVerifier")
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .finish()
    }
}

impl StorageJwtVerifier {
    pub fn new(
        key_provider: Arc<dyn PrincipalKeyProvider>,
        issuer: impl Into<String>,
        audience: impl Into<String>,
    ) -> Self {
        Self {
            key_provider,
            issuer: issuer.into(),
            audience: audience.into(),
        }
    }

    async fn verify(&self, token: &str) -> Result<StorageSessionClaims, String> {
        let (untrusted_header, untrusted_claims) = decode_jwt::<StorageSessionClaims>(token)
            .map_err(|error| format!("invalid JWT: {error:?}"))?;
        if untrusted_claims.iss != self.issuer {
            return Err("JWT issuer mismatch".to_string());
        }
        let candidate = untrusted_header
            .jwk
            .as_ref()
            .ok_or_else(|| "JWT is missing its public JWK".to_string())?;
        let key = self
            .key_provider
            .public_key(&untrusted_claims.iss, untrusted_header.kid, candidate)
            .await?;
        let (header, claims) = verify_jwt::<StorageSessionClaims>(&key, token)
            .map_err(|error| format!("invalid JWT: {error:?}"))?;
        if header.alg != "ES256K" {
            return Err("unsupported JWT algorithm".to_string());
        }
        if header.kid != key.kid {
            return Err("JWT key id mismatch".to_string());
        }
        if claims.iss != self.issuer {
            return Err("JWT issuer mismatch".to_string());
        }
        if claims.aud != self.audience {
            return Err("JWT audience mismatch".to_string());
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_secs() as i64;
        if claims.nbf > now {
            return Err("JWT is not yet valid".to_string());
        }
        if claims.exp <= now {
            return Err("JWT has expired".to_string());
        }
        if claims.sub.trim().is_empty() {
            return Err("JWT subject is empty".to_string());
        }
        Ok(claims)
    }
}

impl StorageBridge {
    pub async fn new(files_dir: PathBuf) -> Self {
        let _ = tokio::fs::create_dir_all(&files_dir).await;
        let registry_path = files_dir.join("known-hosts.sqlite");
        let registry = Arc::new(
            libsql::Builder::new_local(registry_path.to_string_lossy().as_ref())
                .build()
                .await
                .expect("storage known-host registry should initialize"),
        );

        let conn = registry.connect().expect("storage registry connection");
        let _ = conn
            .execute(
                "CREATE TABLE IF NOT EXISTS known_hosts (host TEXT PRIMARY KEY, created_at INTEGER NOT NULL DEFAULT (unixepoch())) STRICT;",
                libsql::params![],
            )
            .await;

        let bridge = Self {
            runtime: StorageIrohRuntime::new(),
            storage: Arc::new(StorageService::new(files_dir)),
            url: Arc::new(tokio::sync::Mutex::new(String::new())),
            registry,
            jwt_verifier: None,
        };

        let _ = bridge.remember_host(STORAGE_BRIDGE_HOST).await;

        bridge
    }

    pub fn with_jwt_verifier(mut self, verifier: StorageJwtVerifier) -> Self {
        self.jwt_verifier = Some(Arc::new(verifier));
        self
    }

    pub async fn remember_host(&self, host: &str) -> Result<(), String> {
        let host = host.trim();
        if host.is_empty() {
            return Ok(());
        }

        let conn = self.registry.connect().map_err(|err| err.to_string())?;
        conn.execute(
            "INSERT OR IGNORE INTO known_hosts (host) VALUES (?)",
            libsql::params![host],
        )
        .await
        .map_err(|err| err.to_string())?;

        Ok(())
    }

    pub async fn is_host_known(&self, host: &str) -> Result<bool, String> {
        let host = host.trim();
        if host.is_empty() {
            return Ok(false);
        }

        let conn = self.registry.connect().map_err(|err| err.to_string())?;
        let mut rows = conn
            .query(
                "SELECT 1 FROM known_hosts WHERE host = ? LIMIT 1",
                libsql::params![host],
            )
            .await
            .map_err(|err| err.to_string())?;

        Ok(rows.next().await.map_err(|err| err.to_string())?.is_some())
    }

    pub async fn validate_session(
        &self,
        host: &str,
        authorization: Option<&str>,
    ) -> Result<StorageService, String> {
        let token = authorization
            .ok_or_else(|| "missing bearer token".to_string())?
            .strip_prefix("Bearer ")
            .ok_or_else(|| "authorization header must use Bearer scheme".to_string())?;

        if !self.is_host_known(host).await? {
            return Err(format!("unknown host: {host}"));
        }

        let verifier = self
            .jwt_verifier
            .as_ref()
            .ok_or_else(|| "storage bridge JWT verifier is not configured".to_string())?;
        let claims = verifier.verify(token).await?;

        let session_storage = self
            .storage
            .for_session(&claims.sub)
            .map_err(|error| format!("invalid session root: {error}"))?;

        Ok(session_storage)
    }

    pub async fn url(&self) -> String {
        self.url.lock().await.clone()
    }

    async fn handle_session_request(
        &self,
        storage: &StorageService,
        request: StorageRequest,
    ) -> Result<StorageResponse, String> {
        log::info!("storage bridge dispatching request: {request:?}");

        let result = match request {
            StorageRequest::ReadFile { path } => {
                let bytes = storage.read_file(&path).await.map_err(|e| e.to_string())?;
                let content = String::from_utf8(bytes).map_err(|e| e.to_string())?;
                Ok(StorageResponse::success_payload(content))
            }
            StorageRequest::WriteFile { path, content } => {
                storage
                    .write_file(&path, content.as_bytes())
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(StorageResponse::success_empty())
            }
            StorageRequest::ListDir { path } => {
                let entries = storage.list_dir(&path).await.map_err(|e| e.to_string())?;
                Ok(StorageResponse::success_payload(json!(entries)))
            }
            StorageRequest::CreateDir { path } => {
                storage.create_dir(&path).await.map_err(|e| e.to_string())?;
                Ok(StorageResponse::success_empty())
            }
            StorageRequest::DeletePath { path } => {
                storage
                    .delete_path(&path)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(StorageResponse::success_empty())
            }
            StorageRequest::RenamePath { from, to } => {
                storage
                    .rename_path(&from, &to)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(StorageResponse::success_empty())
            }
            StorageRequest::ExistsPath { path } => {
                let exists = storage.exists(&path).await.map_err(|e| e.to_string())?;
                Ok(StorageResponse::success_payload(json!(exists)))
            }
            other_request => {
                let event = match other_request {
                    StorageRequest::AddDevice { device_id } => {
                        self.runtime.add_device(&device_id).await?
                    }
                    StorageRequest::RemoveDevice { device_id } => {
                        self.runtime.remove_device(&device_id).await?
                    }
                    StorageRequest::ConnectPeer { peer_id } => {
                        self.runtime.connect_peer(&peer_id).await?
                    }
                    StorageRequest::SyncPeer { peer_id } => {
                        self.runtime.sync_peer(&peer_id).await?
                    }
                    StorageRequest::SendMessage { peer_id, payload } => {
                        self.runtime.send_message(&peer_id, &payload).await?
                    }
                    StorageRequest::CloseSession { peer_id } => {
                        self.runtime.close_session(&peer_id).await?
                    }
                    _ => unreachable!(),
                };

                Ok(StorageResponse::success_event(event))
            }
        };

        match &result {
            Ok(response) => log::info!("storage bridge request completed: {response:?}"),
            Err(error) => log::error!("storage bridge request failed: {error}"),
        }
        result
    }

    pub async fn handle_request(&self, request: StorageRequest) -> Result<StorageResponse, String> {
        self.handle_session_request(&self.storage, request).await
    }

    fn build_server_config(data_dir: &Path) -> Result<Arc<rustls::ServerConfig>, String> {
        let (cert_path, key_path, _) = server_cert_paths(data_dir, STORAGE_BRIDGE_HOST);
        let cert_der = fs::read(&cert_path).map_err(|err| err.to_string())?;
        let key_der = fs::read(&key_path).map_err(|err| err.to_string())?;
        let ca_der = fs::read(ca_der_path(data_dir)).map_err(|err| err.to_string())?;

        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(cert_der), CertificateDer::from(ca_der)],
                PrivateKeyDer::from(PrivatePkcs8KeyDer::from(key_der)),
            )
            .map_err(|err| err.to_string())?;

        Ok(Arc::new(server_config))
    }

    pub async fn start_server(self, data_dir: &Path) -> Result<String, String> {
        let state = Arc::new(self.clone());

        let app = Router::new()
            .route("/", get(Self::bridge_handler))
            .route("/trust", get(Self::trust_page_handler))
            .with_state(state);

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|err| err.to_string())?;

        let port = listener.local_addr().map_err(|err| err.to_string())?.port();

        let bridge_url = bridge_url(STORAGE_BRIDGE_HOST, port);
        {
            let mut current = self.url.lock().await;
            *current = bridge_url.clone();
        }

        let tls_listener = TlsListener {
            inner: listener,
            acceptor: TlsAcceptor::from(Self::build_server_config(data_dir)?),
        };

        axum::serve(tls_listener, app)
            .await
            .map_err(|err| err.to_string())?;

        Ok(bridge_url)
    }

    async fn bridge_handler(
        ws: WebSocketUpgrade,
        State(bridge): State<Arc<Self>>,
    ) -> impl IntoResponse {
        ws.on_upgrade(|socket| Self::handle_bridge_socket(socket, bridge))
    }

    async fn trust_page_handler() -> Html<&'static str> {
        Html(TRUST_PAGE)
    }

    async fn handle_bridge_socket(mut socket: WebSocket, bridge: Arc<Self>) {
        let mut event_rx = bridge.runtime.subscribe_events();

        loop {
            tokio::select! {
                msg = socket.recv() => {
                    let Some(msg) = msg else {
                        break;
                    };

                    let Ok(Message::Text(raw)) = msg else {
                        continue;
                    };

                    log::info!("storage bridge received websocket frame: {raw}");

                    match serde_json::from_str::<BridgeMessage>(&raw) {
                        Ok(bridge_msg) => match bridge_msg {
                            BridgeMessage::Request {
                                request,
                                request_id,
                                authorization,
                            } => {
                                let host = STORAGE_BRIDGE_HOST;
                                let session_storage = match bridge
                                    .validate_session(host, authorization.as_deref())
                                    .await
                                {
                                    Ok(storage) => storage,
                                    Err(err) => {
                                        let response = StorageResponse::error(err);
                                        let payload = serde_json::to_string(&BridgeMessage::Response {
                                            response,
                                            request_id,
                                        })
                                        .unwrap_or_else(|_| "{\"ok\":false,\"error\":\"serialization failed\"}".to_string());
                                        if socket.send(Message::Text(payload.into())).await.is_err() {
                                            break;
                                        }
                                        continue;
                                    }
                                };

                                let response = bridge
                                    .handle_session_request(&session_storage, request)
                                    .await;

                                let response = match response {
                                    Ok(r) => r,
                                    Err(err) => StorageResponse::error(err),
                                };

                                let payload = match serde_json::to_string(&BridgeMessage::Response {
                                    response,
                                    request_id,
                                }) {
                                    Ok(payload) => payload,
                                    Err(_) => {
                                        let fallback = "{\"ok\":false,\"error\":\"serialization failed\"}".to_string();
                                        if socket
                                            .send(Message::Text(fallback.into()))
                                            .await
                                            .is_err()
                                        {
                                            break;
                                        }
                                        continue;
                                    }
                                };

                                log::info!("storage bridge sending websocket response: {payload}");
                                if socket.send(Message::Text(payload.into())).await.is_err() {
                                    log::error!("storage bridge failed to send websocket response");
                                    break;
                                }
                            }
                            BridgeMessage::Response { .. } => {
                                // Unexpected response from client, ignore
                            }
                            BridgeMessage::Event(_) => {
                                // Client should not send events, ignore
                            }
                        },
                        Err(error) => {
                            log::error!("storage bridge rejected websocket frame: {error}");
                        }
                    }
                }
                Ok(event) = event_rx.recv() => {
                    let payload = match serde_json::to_string(&BridgeMessage::Event(event)) {
                        Ok(p) => p,
                        Err(_) => continue,
                    };

                    if socket.send(Message::Text(payload.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
}

pub type StorageBridgeState = tokio::sync::Mutex<StorageBridge>;

pub fn bridge_trust_prompted_path(data_dir: &Path) -> PathBuf {
    data_dir.join("bridge-trust-prompted")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bridge_tracks_known_hosts_in_sqlite_registry() {
        let temp_dir =
            std::env::temp_dir().join(format!("storage-host-registry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let bridge = StorageBridge::new(temp_dir.join("files")).await;
        bridge.remember_host("trusted.example").await.unwrap();

        assert!(bridge.is_host_known("trusted.example").await.unwrap());
        assert!(!bridge.is_host_known("unknown.example").await.unwrap());
    }

    #[tokio::test]
    async fn ensure_storage_bridge_certificate_creates_and_reuses_files() {
        let temp_dir =
            std::env::temp_dir().join(format!("storage-cert-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();

        let cert_path = ensure_storage_bridge_certificate(&temp_dir).await.unwrap();
        assert!(cert_path.exists());
        assert!(ca_key_path(&temp_dir).exists());
        assert!(ca_der_path(&temp_dir).exists());

        let (server_cert, server_key, server_pem) =
            server_cert_paths(&temp_dir, STORAGE_BRIDGE_HOST);
        assert!(server_cert.exists());
        assert!(server_key.exists());
        assert!(server_pem.exists());

        let pem_content_first = std::fs::read_to_string(&cert_path).unwrap();

        // Second call should reuse the existing certs
        let cert_path_second = ensure_storage_bridge_certificate(&temp_dir).await.unwrap();
        assert_eq!(cert_path, cert_path_second);
        let pem_content_second = std::fs::read_to_string(&cert_path_second).unwrap();
        assert_eq!(pem_content_first, pem_content_second);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn bridge_message_uses_camel_case_request_id() {
        let message = serde_json::json!({
            "type": "writeFile",
            "path": "example/hello.txt",
            "content": "Hello",
            "requestId": 1,
        });
        let parsed = serde_json::from_value::<BridgeMessage>(message).unwrap();
        assert!(matches!(
            parsed,
            BridgeMessage::Request { request_id: 1, .. }
        ));
    }

    #[test]
    fn bridge_url_uses_host_and_port() {
        assert_eq!(
            bridge_url(STORAGE_BRIDGE_HOST, 33233),
            "wss://storage.localhost:33233"
        );
    }

    #[test]
    fn bridge_trust_url_converts_wss_to_https() {
        assert_eq!(
            bridge_trust_url("wss://storage.localhost:33233"),
            Some("https://storage.localhost:33233/trust".to_string())
        );
    }

    #[test]
    fn server_cert_paths_include_host_suffix() {
        let dir = Path::new("/tmp/storage-data");
        let (cert, key, pem) = server_cert_paths(dir, STORAGE_BRIDGE_HOST);
        assert_eq!(
            cert,
            dir.join("storage-bridge-server-storage-localhost.der")
        );
        assert_eq!(key, dir.join("storage-bridge-server-storage-localhost.key"));
        assert_eq!(pem, dir.join("storage-bridge-server-storage-localhost.pem"));
    }
}
