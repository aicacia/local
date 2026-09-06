use alloc::vec::Vec;
use core::fmt;

use chacha20poly1305::{
    XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit, Payload},
};

use crate::Storage;

const VERSION: u8 = 1;
const NONCE_SIZE: usize = 24;

#[derive(Debug)]
pub enum EncryptedStorageError<E> {
    Storage(E),
    InvalidCiphertext,
    Encrypt,
    Decrypt,
    Random,
}

impl<E: fmt::Display> fmt::Display for EncryptedStorageError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(error) => error.fmt(formatter),
            Self::InvalidCiphertext => formatter.write_str("invalid encrypted storage ciphertext"),
            Self::Encrypt => formatter.write_str("failed to encrypt storage content"),
            Self::Decrypt => formatter.write_str("failed to decrypt storage content"),
            Self::Random => formatter.write_str("failed to generate an encryption nonce"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for EncryptedStorageError<E> {}

pub struct EncryptedStorage<S> {
    inner: S,
    cipher: XChaCha20Poly1305,
}

impl<S> EncryptedStorage<S> {
    #[must_use]
    pub fn new(inner: S, key: [u8; 32]) -> Self {
        Self {
            inner,
            cipher: XChaCha20Poly1305::new((&key).into()),
        }
    }

    #[must_use]
    pub fn into_inner(self) -> S {
        self.inner
    }

    fn encrypt(&self, path: &str, content: &[u8]) -> Result<Vec<u8>, EncryptedStorageError<()>> {
        let mut nonce = [0_u8; NONCE_SIZE];
        getrandom::fill(&mut nonce).map_err(|_| EncryptedStorageError::Random)?;
        let ciphertext = self
            .cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: content,
                    aad: path.as_bytes(),
                },
            )
            .map_err(|_| EncryptedStorageError::Encrypt)?;
        let mut encrypted = Vec::with_capacity(1 + NONCE_SIZE + ciphertext.len());
        encrypted.push(VERSION);
        encrypted.extend_from_slice(&nonce);
        encrypted.extend_from_slice(&ciphertext);
        Ok(encrypted)
    }

    fn decrypt(&self, path: &str, encrypted: &[u8]) -> Result<Vec<u8>, EncryptedStorageError<()>> {
        let Some((&version, encrypted)) = encrypted.split_first() else {
            return Err(EncryptedStorageError::InvalidCiphertext);
        };
        if version != VERSION || encrypted.len() < NONCE_SIZE {
            return Err(EncryptedStorageError::InvalidCiphertext);
        }
        let (nonce, ciphertext) = encrypted.split_at(NONCE_SIZE);
        self.cipher
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: path.as_bytes(),
                },
            )
            .map_err(|_| EncryptedStorageError::Decrypt)
    }
}

impl<S: Storage + Send + Sync> Storage for EncryptedStorage<S> {
    type Error = EncryptedStorageError<S::Error>;

    async fn read(&self, path: &str) -> Result<Vec<u8>, Self::Error> {
        let encrypted = self
            .inner
            .read(path)
            .await
            .map_err(EncryptedStorageError::Storage)?;
        self.decrypt(path, &encrypted).map_err(map_crypto_error)
    }

    async fn write(&mut self, path: &str, content: &[u8]) -> Result<(), Self::Error> {
        let encrypted = self.encrypt(path, content).map_err(map_crypto_error)?;
        self.inner
            .write(path, &encrypted)
            .await
            .map_err(EncryptedStorageError::Storage)
    }

    async fn append(&mut self, path: &str, content: &[u8]) -> Result<(), Self::Error> {
        let mut existing = self.read(path).await?;
        existing.extend_from_slice(content);
        self.write(path, &existing).await
    }

    async fn remove(&mut self, path: &str) -> Result<(), Self::Error> {
        self.inner
            .remove(path)
            .await
            .map_err(EncryptedStorageError::Storage)
    }

    async fn rename(&mut self, from: &str, to: &str) -> Result<(), Self::Error> {
        // ponytail: generic Storage has no atomic replace; add it if crash-safe rename is required.
        let content = self.read(from).await?;
        self.write(to, &content).await?;
        self.remove(from).await
    }

    async fn list(&self, path: &str) -> Result<Vec<String>, Self::Error> {
        self.inner
            .list(path)
            .await
            .map_err(EncryptedStorageError::Storage)
    }
}

fn map_crypto_error<E>(error: EncryptedStorageError<()>) -> EncryptedStorageError<E> {
    match error {
        EncryptedStorageError::Storage(()) => unreachable!(),
        EncryptedStorageError::InvalidCiphertext => EncryptedStorageError::InvalidCiphertext,
        EncryptedStorageError::Encrypt => EncryptedStorageError::Encrypt,
        EncryptedStorageError::Decrypt => EncryptedStorageError::Decrypt,
        EncryptedStorageError::Random => EncryptedStorageError::Random,
    }
}

#[cfg(test)]
mod tests {
    use crate::{EncryptedStorage, InMemoryStorage, Storage};

    #[tokio::test]
    async fn encrypts_content_and_binds_it_to_its_path() {
        let mut storage = EncryptedStorage::new(InMemoryStorage::new(), [7; 32]);
        storage.write("vault/data", b"secret").await.unwrap();
        storage.append("vault/data", b" value").await.unwrap();
        storage.rename("vault/data", "vault/renamed").await.unwrap();

        assert_eq!(
            storage.read("vault/renamed").await.unwrap(),
            b"secret value"
        );

        let mut inner = storage.into_inner();
        let ciphertext = inner.read("vault/renamed").await.unwrap();
        assert_ne!(ciphertext, b"secret value");
        inner.rename("vault/renamed", "vault/moved").await.unwrap();

        let storage = EncryptedStorage::new(inner, [7; 32]);
        assert!(storage.read("vault/moved").await.is_err());
    }
}
