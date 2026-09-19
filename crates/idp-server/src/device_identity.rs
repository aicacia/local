use std::io;

use idp_service::repo::RawKeyringRepo;
use iroh::{Endpoint, SecretKey, endpoint::presets};
use iroh_chain::TUNNEL_ALPN;

use crate::DeviceIdentity;

const KEYRING_SERVICE: &str = "local.device-identity";
const KEYRING_ENTRY: &str = "device";

pub async fn open() -> io::Result<DeviceIdentity> {
    let keyring = RawKeyringRepo::new(KEYRING_SERVICE);
    let secret_key = match keyring
        .load("", "", KEYRING_ENTRY)
        .map_err(io::Error::other)?
    {
        Some(bytes) => SecretKey::from_bytes(
            &bytes
                .try_into()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid Iroh key"))?,
        ),
        None => {
            let secret_key = SecretKey::generate();
            keyring
                .store("", "", KEYRING_ENTRY, &secret_key.to_bytes())
                .map_err(io::Error::other)?;
            secret_key
        }
    };
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key.clone())
        .alpns(vec![TUNNEL_ALPN.to_vec()])
        .bind()
        .await
        .map_err(io::Error::other)?;
    Ok(DeviceIdentity::new(endpoint, secret_key))
}

pub fn delete() -> io::Result<()> {
    RawKeyringRepo::new(KEYRING_SERVICE)
        .delete("", "", KEYRING_ENTRY)
        .map_err(io::Error::other)
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
