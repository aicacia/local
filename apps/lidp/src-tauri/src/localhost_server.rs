use std::{
    fs, io,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::Router;
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, Issuer, KeyPair, SanType,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use crate::localhost_trust::install_ca_to_user_trust_store;

pub fn localhost_ca_cert_path(data_dir: &Path) -> PathBuf {
    data_dir.join("localhost-ca.pem")
}

fn localhost_ca_key_path(data_dir: &Path) -> PathBuf {
    data_dir.join("localhost-ca.key")
}

fn localhost_ca_der_path(data_dir: &Path) -> PathBuf {
    data_dir.join("localhost-ca.der")
}

fn localhost_server_cert_paths(data_dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
    (
        data_dir.join("localhost-server.der"),
        data_dir.join("localhost-server.key"),
        data_dir.join("localhost-server.pem"),
    )
}

fn generate_ca_certificate(ca_key: &KeyPair) -> io::Result<rcgen::Certificate> {
    let mut ca_params = CertificateParams::new(vec![]).map_err(io::Error::other)?;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.distinguished_name = DistinguishedName::new();
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "Localhost CA");

    ca_params.self_signed(ca_key).map_err(io::Error::other)
}

async fn load_or_create_ca(data_dir: &Path) -> io::Result<(KeyPair, bool)> {
    let key_path = localhost_ca_key_path(data_dir);

    if fs::exists(&key_path)? {
        let key = fs::read_to_string(&key_path)?;
        return Ok((KeyPair::from_pem(&key).map_err(io::Error::other)?, false));
    }

    let key = KeyPair::generate().map_err(io::Error::other)?;
    fs::write(&key_path, key.serialize_pem())?;

    Ok((key, true))
}

async fn ensure_ca_certificate_pem(
    data_dir: &Path,
    ca_key: &KeyPair,
) -> io::Result<(String, bool)> {
    let ca_pem_path = localhost_ca_cert_path(data_dir);
    let ca_der_path = localhost_ca_der_path(data_dir);

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

async fn load_or_create_server_cert(data_dir: &Path, ca_key: &KeyPair) -> io::Result<()> {
    let (cert_path, key_path, cert_pem_path) = localhost_server_cert_paths(data_dir);

    if fs::exists(&cert_path)? && fs::exists(&key_path)? && fs::exists(&cert_pem_path)? {
        return Ok(());
    }

    let mut params =
        CertificateParams::new(vec!["localhost".to_string()]).map_err(io::Error::other)?;
    params
        .subject_alt_names
        .push(SanType::IpAddress(Ipv4Addr::LOCALHOST.into()));
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, "localhost");
    params.is_ca = IsCa::NoCa;

    let key = KeyPair::generate().map_err(io::Error::other)?;

    let ca_params = {
        let mut params = CertificateParams::new(vec![]).map_err(io::Error::other)?;
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, "Localhost CA");
        params
    };

    let ca_issuer = Issuer::new(ca_params, ca_key);
    let cert = params
        .signed_by(&key, &ca_issuer)
        .map_err(io::Error::other)?;

    let cert_der = cert.der().to_vec();
    let key_der = key.serialize_der();
    let cert_pem = cert.pem();

    fs::write(&cert_path, &cert_der)?;
    fs::write(&key_path, &key_der)?;
    fs::write(cert_pem_path, cert_pem.as_bytes())?;

    Ok(())
}

pub async fn ensure_localhost_certificate(data_dir: &Path) -> io::Result<PathBuf> {
    let cert_path = localhost_ca_cert_path(data_dir);
    let (ca, is_new_ca) = load_or_create_ca(data_dir).await?;
    let (_ca_pem, is_new_pem) = ensure_ca_certificate_pem(data_dir, &ca).await?;
    load_or_create_server_cert(data_dir, &ca).await?;

    if is_new_ca || is_new_pem {
        let _ = install_ca_to_user_trust_store(&cert_path);
    }

    Ok(cert_path)
}

fn build_server_config(data_dir: &Path) -> Result<Arc<rustls::ServerConfig>, String> {
    let (cert_path, key_path, _) = localhost_server_cert_paths(data_dir);
    let cert_der = fs::read(&cert_path).map_err(|err| err.to_string())?;
    let key_der = fs::read(&key_path).map_err(|err| err.to_string())?;
    let ca_der = fs::read(localhost_ca_der_path(data_dir)).map_err(|err| err.to_string())?;

    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(cert_der), CertificateDer::from(ca_der)],
            PrivateKeyDer::from(PrivatePkcs8KeyDer::from(key_der)),
        )
        .map_err(|err| err.to_string())?;

    Ok(Arc::new(server_config))
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
                        log::warn!("localhost TLS accept failed: {err}");
                        continue;
                    }
                },
                Err(err) => {
                    log::warn!("localhost TCP accept failed: {err}");
                    continue;
                }
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.inner.local_addr()
    }
}

pub fn localhost_server_base_url(port: u16) -> String {
    format!("https://localhost:{port}")
}

pub async fn reserve_localhost_listener(data_dir: &Path) -> Result<(TcpListener, u16), String> {
    ensure_localhost_certificate(data_dir)
        .await
        .map_err(|err| err.to_string())?;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|err| err.to_string())?;
    let port = listener.local_addr().map_err(|err| err.to_string())?.port();

    Ok((listener, port))
}

pub fn start_unified_localhost_server(router: Router, listener: TcpListener, data_dir: &Path) {
    let tls_listener = match build_server_config(data_dir) {
        Ok(server_config) => TlsListener {
            inner: listener,
            acceptor: TlsAcceptor::from(server_config),
        },
        Err(err) => {
            log::error!("failed to build localhost TLS config: {err}");
            return;
        }
    };

    let app = router;
    tauri::async_runtime::spawn(async move {
        if let Err(err) = axum::serve(tls_listener, app).await {
            log::error!("unified localhost server failed: {err}");
        }
    });
}
