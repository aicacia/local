#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

use crate::contract::JwkPublicParameters;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
#[serde(tag = "kty")]
pub enum JwkPrivateParameters {
    Rsa {
        n: String,
        e: String,
        d: String,
        p: String,
        q: String,
    },
    Ec {
        crv: String,
        x: String,
        y: String,
        d: String,
    },
    Opk {
        crv: String,
        x: String,
        d: String,
    },
    Oct {
        k: String,
    },
}

impl From<JwkPrivateParameters> for JwkPublicParameters {
    fn from(val: JwkPrivateParameters) -> Self {
        match val {
            JwkPrivateParameters::Rsa { n, e, .. } => JwkPublicParameters::Rsa { n, e },
            JwkPrivateParameters::Ec { crv, x, y, .. } => JwkPublicParameters::Ec { crv, x, y },
            JwkPrivateParameters::Opk { crv, x, .. } => JwkPublicParameters::Opk { crv, x },
            JwkPrivateParameters::Oct { .. } => {
                unreachable!("Octet keys are not supported in Jwk Public Parameters")
            }
        }
    }
}
