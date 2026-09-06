use iroh::{Endpoint, EndpointId, SecretKey, endpoint::presets};
use lidp_service::repo::RawKeyringRepo;

const DEVICE_IROH_KEY: &str = "lidp-device-iroh-key";
const LIDP_VAULT_ALPN: &[u8] = b"lidp-vault/1";

pub struct DeviceIdentity {
    endpoint: Endpoint,
}

impl DeviceIdentity {
    pub async fn open(keyring_service: &str) -> Result<Self, String> {
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
            .secret_key(secret_key)
            .alpns(vec![LIDP_VAULT_ALPN.to_vec()])
            .bind()
            .await
            .map_err(|error| error.to_string())?;
        Ok(Self { endpoint })
    }

    pub fn endpoint_id(&self) -> EndpointId {
        self.endpoint.id()
    }
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
