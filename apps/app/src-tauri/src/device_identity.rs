use idp_service::repo::RawKeyringRepo;
use iroh::{Endpoint, SecretKey, endpoint::presets};
use iroh_chain::TUNNEL_ALPN;

const DEVICE_IROH_KEY: &str = "idp-device-iroh-key";

pub use idp_server::DeviceIdentity;

pub async fn open(keyring_service: &str) -> Result<DeviceIdentity, String> {
    let keyring = RawKeyringRepo::new(keyring_service);
    let secret_key = match keyring
        .load("", "", DEVICE_IROH_KEY)
        .map_err(|error| error.to_string())?
    {
        Some(bytes) => SecretKey::from_bytes(
            &bytes
                .try_into()
                .map_err(|_| "invalid Iroh device key length")?,
        ),
        None => {
            let secret_key = SecretKey::generate();
            keyring
                .store("", "", DEVICE_IROH_KEY, &secret_key.to_bytes())
                .map_err(|error| error.to_string())?;
            secret_key
        }
    };
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key.clone())
        .alpns(vec![TUNNEL_ALPN.to_vec()])
        .bind()
        .await
        .map_err(|error| error.to_string())?;
    Ok(DeviceIdentity::new(endpoint, secret_key))
}

#[cfg(test)]
mod tests {
    use iroh::SecretKey;

    #[test]
    fn restores_device_key_bytes() {
        let key = SecretKey::generate();
        assert_eq!(
            SecretKey::from_bytes(&key.to_bytes()).public(),
            key.public()
        );
    }
}
