#[cfg(not(feature = "std"))]
use alloc::{
    format,
    string::{String, ToString},
};

use base64::{
    Engine,
    engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD},
};
use idp_model::contract::{
    ErrorCode, ErrorResponse, ErrorResponseResult, JwkPrivate, JwkPrivateParameters, JwkPublic,
    JwkPublicParameters,
};
use k256::ecdsa::{
    Signature, SigningKey, VerifyingKey,
    signature::{Signer, Verifier},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Serialize, Deserialize)]
pub struct JwtHeader {
    pub alg: String,
    pub typ: String,
    pub kid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwk: Option<JwkPublic>,
}

const JWT_HEADER_ALG: &str = "ES256K";
const JWT_HEADER_TYP: &str = "JWT";

pub fn encode_jwt<T>(jwk: &JwkPrivate, claims: &T) -> ErrorResponseResult<String>
where
    T: Serialize,
{
    let header = JwtHeader {
        alg: JWT_HEADER_ALG.to_string(),
        typ: JWT_HEADER_TYP.to_string(),
        kid: jwk.kid.clone(),
        jwk: None,
    };

    let header_encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).map_err(|error| {
        ErrorResponse::new(ErrorCode::ServerError).with_description(error.to_string())
    })?);
    let claims_encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).map_err(|error| {
        ErrorResponse::new(ErrorCode::ServerError).with_description(error.to_string())
    })?);

    let signing_input = format!("{header_encoded}.{claims_encoded}");

    let signing_key = signing_key_from_jwk(jwk)?;
    let signature: Signature = signing_key.sign(signing_input.as_bytes());
    let signature_encoded = URL_SAFE_NO_PAD.encode(signature.to_bytes());

    Ok(format!("{signing_input}.{signature_encoded}"))
}

/// Decodes a JWT and returns the header and claims without verifying the signature.
pub fn decode_jwt<T>(token: &str) -> ErrorResponseResult<(JwtHeader, T)>
where
    T: DeserializeOwned,
{
    let mut parts = token.split('.');
    let header_encoded = parts.next().ok_or_else(|| {
        ErrorResponse::new(ErrorCode::InvalidRequest).with_description("invalid JWT format")
    })?;
    let claims_encoded = parts.next().ok_or_else(|| {
        ErrorResponse::new(ErrorCode::InvalidRequest).with_description("invalid JWT format")
    })?;

    let header_bytes = URL_SAFE_NO_PAD.decode(header_encoded).map_err(|error| {
        ErrorResponse::new(ErrorCode::InvalidRequest)
            .with_description(format!("invalid JWT header encoding: {error}"))
    })?;
    let claims_bytes = URL_SAFE_NO_PAD.decode(claims_encoded).map_err(|error| {
        ErrorResponse::new(ErrorCode::InvalidRequest)
            .with_description(format!("invalid JWT claims encoding: {error}"))
    })?;

    let header: JwtHeader = serde_json::from_slice(&header_bytes).map_err(|error| {
        ErrorResponse::new(ErrorCode::InvalidRequest)
            .with_description(format!("invalid JWT header: {error}"))
    })?;
    let claims: T = serde_json::from_slice(&claims_bytes).map_err(|error| {
        ErrorResponse::new(ErrorCode::InvalidRequest)
            .with_description(format!("invalid JWT claims: {error}"))
    })?;

    Ok((header, claims))
}

/// Verifies a JWT and returns the header and claims if the signature is valid.
pub fn verify_jwt<T>(jwk: &JwkPublic, token: &str) -> ErrorResponseResult<(JwtHeader, T)>
where
    T: DeserializeOwned,
{
    let mut parts = token.split('.');
    let header_encoded = parts.next().ok_or_else(|| {
        ErrorResponse::new(ErrorCode::InvalidRequest).with_description("invalid JWT format")
    })?;
    let claims_encoded = parts.next().ok_or_else(|| {
        ErrorResponse::new(ErrorCode::InvalidRequest).with_description("invalid JWT format")
    })?;
    let signature_encoded = parts.next().ok_or_else(|| {
        ErrorResponse::new(ErrorCode::InvalidRequest).with_description("invalid JWT format")
    })?;

    let header_bytes = URL_SAFE_NO_PAD.decode(header_encoded).map_err(|error| {
        ErrorResponse::new(ErrorCode::InvalidRequest)
            .with_description(format!("invalid JWT header encoding: {error}"))
    })?;
    let claims_bytes = URL_SAFE_NO_PAD.decode(claims_encoded).map_err(|error| {
        ErrorResponse::new(ErrorCode::InvalidRequest)
            .with_description(format!("invalid JWT claims encoding: {error}"))
    })?;
    let signature_bytes = URL_SAFE_NO_PAD.decode(signature_encoded).map_err(|error| {
        ErrorResponse::new(ErrorCode::InvalidRequest)
            .with_description(format!("invalid JWT signature encoding: {error}"))
    })?;

    let header: JwtHeader = serde_json::from_slice(&header_bytes).map_err(|error| {
        ErrorResponse::new(ErrorCode::InvalidRequest)
            .with_description(format!("invalid JWT header: {error}"))
    })?;
    let claims: T = serde_json::from_slice(&claims_bytes).map_err(|error| {
        ErrorResponse::new(ErrorCode::InvalidRequest)
            .with_description(format!("invalid JWT claims: {error}"))
    })?;

    let signature = Signature::from_slice(&signature_bytes).map_err(|error| {
        ErrorResponse::new(ErrorCode::InvalidRequest)
            .with_description(format!("invalid JWT signature: {error}"))
    })?;

    let verifying_key = verifing_key_from_jwt(jwk)?;
    let signing_input = format!("{header_encoded}.{claims_encoded}");

    verifying_key
        .verify(signing_input.as_bytes(), &signature)
        .map_err(|error| {
            ErrorResponse::new(ErrorCode::InvalidRequest)
                .with_description(format!("invalid JWT signature: {error}"))
        })?;

    Ok((header, claims))
}

fn signing_key_from_jwk(jwk: &JwkPrivate) -> ErrorResponseResult<SigningKey> {
    match &jwk.params {
        JwkPrivateParameters::Ec { d, .. } => {
            let private_key_bytes = STANDARD_NO_PAD.decode(d).map_err(|error| {
                ErrorResponse::new(ErrorCode::InvalidClient)
                    .with_description(format!("invalid client private key encoding: {error}"))
            })?;

            SigningKey::from_slice(&private_key_bytes).map_err(|error| {
                ErrorResponse::new(ErrorCode::InvalidClient)
                    .with_description(format!("invalid client private key: {error}"))
            })
        }
        _ => Err(ErrorResponse::new(ErrorCode::InvalidClient)
            .with_description("client signing key must be an EC key")),
    }
}

fn verifing_key_from_jwt(jwk: &JwkPublic) -> ErrorResponseResult<VerifyingKey> {
    match &jwk.params {
        JwkPublicParameters::Ec { x, y, .. } => {
            let x_bytes = STANDARD_NO_PAD.decode(x).map_err(|error| {
                ErrorResponse::new(ErrorCode::InvalidClient)
                    .with_description(format!("invalid client public key encoding: {error}"))
            })?;
            let y_bytes = STANDARD_NO_PAD.decode(y).map_err(|error| {
                ErrorResponse::new(ErrorCode::InvalidClient)
                    .with_description(format!("invalid client public key encoding: {error}"))
            })?;

            let public_key_bytes =
                [b"\x04".as_slice(), x_bytes.as_slice(), y_bytes.as_slice()].concat();

            VerifyingKey::from_sec1_bytes(&public_key_bytes).map_err(|error| {
                ErrorResponse::new(ErrorCode::InvalidClient)
                    .with_description(format!("invalid client public key: {error}"))
            })
        }
        _ => Err(ErrorResponse::new(ErrorCode::InvalidClient)
            .with_description("client verifying key must be an EC key")),
    }
}
