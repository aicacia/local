use core::{convert::TryFrom, str::FromStr};

use bip32::{ChildNumber, DerivationPath, XPrv};
use bip39::Mnemonic;
use k256::ecdsa::VerifyingKey as ECDSAVerifyingKey;
use zeroize::Zeroizing;

use crate::{DerivedKey, KeyError, KeyResult};

#[derive(Debug)]
pub struct MasterKey {
    key: XPrv,
}

impl TryFrom<Mnemonic> for MasterKey {
    type Error = KeyError;

    fn try_from(mnemonic: Mnemonic) -> Result<Self, Self::Error> {
        Self::from_mnemonic(mnemonic)
    }
}

impl MasterKey {
    pub fn from_mnemonic_with_passphrase<S>(
        mnemonic: Mnemonic,
        normalized_passphrase: S,
    ) -> KeyResult<Self>
    where
        S: AsRef<str>,
    {
        let seed = Zeroizing::new(
            mnemonic
                .to_seed_normalized(normalized_passphrase.as_ref())
                .to_vec(),
        );
        let key = XPrv::new(seed.as_slice())?;

        Ok(Self { key })
    }

    pub fn from_entropy_with_passphrase<T, S>(
        entropy: T,
        normalized_passphrase: S,
    ) -> KeyResult<Self>
    where
        T: AsRef<[u8]>,
        S: AsRef<str>,
    {
        Self::from_mnemonic_with_passphrase(
            Mnemonic::from_entropy(entropy.as_ref())?,
            normalized_passphrase,
        )
    }

    pub fn from_entropy<T>(entropy: T) -> KeyResult<Self>
    where
        T: AsRef<[u8]>,
    {
        Self::from_entropy_with_passphrase(entropy, "")
    }

    pub fn entropy() -> KeyResult<Zeroizing<[u8; 32]>> {
        let mut entropy = Zeroizing::new([0u8; 32]);
        getrandom::fill(entropy.as_mut())?;
        Ok(entropy)
    }

    pub fn from_mnemonic(mnemonic: Mnemonic) -> KeyResult<Self> {
        Self::from_mnemonic_with_passphrase(mnemonic, "")
    }

    pub fn from_xprv<S>(xprv: S) -> KeyResult<Self>
    where
        S: AsRef<str>,
    {
        let key = XPrv::from_str(xprv.as_ref())?;
        Ok(Self { key })
    }

    pub fn key(&self) -> &XPrv {
        &self.key
    }

    pub fn derive_from_child_number(&self, child_number: ChildNumber) -> KeyResult<DerivedKey> {
        let key = self.key.derive_child(child_number)?;

        let mut derivation_path = DerivationPath::default();
        derivation_path.push(child_number);

        Ok(DerivedKey::new(key, derivation_path))
    }

    pub fn derive_from_index_and_hardened(
        &self,
        index: u32,
        hardened: bool,
    ) -> KeyResult<DerivedKey> {
        self.derive_from_child_number(ChildNumber::new(index, hardened)?)
    }

    pub fn derive<S>(&self, path: S) -> KeyResult<DerivedKey>
    where
        S: AsRef<str>,
    {
        let derivation_path = DerivationPath::from_str(path.as_ref())?;

        let mut key = self.key.clone();

        for child in derivation_path.as_ref() {
            key = key.derive_child(*child)?;
        }

        Ok(DerivedKey::new(key, derivation_path))
    }

    pub fn verify_derived_key(&self, child: DerivedKey) -> KeyResult<bool> {
        let mut key = self.key.clone();

        for child in child.derivation_path().as_ref() {
            key = key.derive_child(*child)?;
        }

        Ok(key.private_key() == child.key().private_key())
    }

    pub fn verify_ecdsa_key<S>(&self, path: S, child: &ECDSAVerifyingKey) -> KeyResult<bool>
    where
        S: AsRef<str>,
    {
        let derivation_path = DerivationPath::from_str(path.as_ref())?;

        let mut xpub = self.key.public_key();

        for child in derivation_path.as_ref() {
            if child.is_hardened() {
                return Ok(false);
            }

            match xpub.derive_child(*child) {
                Ok(next) => xpub = next,
                Err(_) => return Ok(false),
            }
        }

        Ok(xpub.public_key() == child)
    }
}

impl From<MasterKey> for DerivedKey {
    fn from(value: MasterKey) -> Self {
        Self::new(value.key, DerivationPath::default())
    }
}
