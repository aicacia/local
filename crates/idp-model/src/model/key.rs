use core::str::FromStr;

#[cfg(not(feature = "std"))]
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
use chrono::{DateTime, Utc};
use key::{DerivationPath, DerivedKey, KeyResult};

use crate::contract::{
    EntityType, JwkPrivate, JwkPrivateParameters, JwkPublic, JwkPublicParameters, JwsAlgorithm,
    KeyUse,
};
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Key {
    pub id: u32,

    pub parent_id: Option<u32>,

    #[serde(with = "super::sql_enum::entity_type")]
    pub entity_type: EntityType,
    pub entity_id: i64,

    #[serde(deserialize_with = "super::none_to_default")]
    pub derivation_path: String,
    pub name: String,
    pub hardened: bool,

    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub revoked_at: Option<DateTime<Utc>>,
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub expires_at: Option<DateTime<Utc>>,

    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub updated_at: DateTime<Utc>,
}

impl Key {
    pub fn to_jwk_private(&self, derived_key: &DerivedKey) -> KeyResult<JwkPrivate> {
        let (x, y, d) = derived_key.to_xyd()?;

        let jwt = JwkPrivate {
            r#use: KeyUse::Signature,
            kid: self.id,
            alg: JwsAlgorithm::EdDSA,
            params: JwkPrivateParameters::Ec {
                crv: "secp256k1".to_string(),
                x: STANDARD_NO_PAD.encode(x),
                y: STANDARD_NO_PAD.encode(y),
                d: STANDARD_NO_PAD.encode(d),
            },
        };

        Ok(jwt)
    }

    pub fn to_jwk_public(&self, derived_key: &DerivedKey) -> KeyResult<JwkPublic> {
        let (x, y) = derived_key.to_xy()?;

        let jwt = JwkPublic {
            r#use: KeyUse::Signature,
            kid: self.id,
            alg: JwsAlgorithm::EdDSA,
            params: JwkPublicParameters::Ec {
                crv: "secp256k1".to_string(),
                x: STANDARD_NO_PAD.encode(x),
                y: STANDARD_NO_PAD.encode(y),
            },
        };

        Ok(jwt)
    }

    pub fn derivation_path(&self) -> KeyResult<DerivationPath> {
        let derivation_path = DerivationPath::from_str(&self.derivation_path)?;
        Ok(derivation_path)
    }

    pub fn build_derivation_path(
        parent_derivation_path: Option<&str>,
        key_id: u32,
        hardened: bool,
    ) -> String {
        let mut path = String::new();

        if let Some(parent_path) = parent_derivation_path {
            path.push_str(parent_path);
        } else {
            path.push('m');
        }

        if hardened {
            // FIXME: we should make the key if fits in the hardened range, but for now we just append a `'` to indicate it's hardened
            path.push_str(&format!("/{}'", key_id));
        } else {
            path.push_str(&format!("/{}", key_id));
        }

        path
    }
}
