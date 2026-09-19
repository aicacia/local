use idp_model::{
    contract::{
        ErrorCode, ErrorResponse, ErrorResponseResult, JwkPublic, TunnelAuthorizationClaims,
    },
    model::Id,
};

use super::verify_jwt;

pub fn verify_tunnel_authorization(
    jwk: &JwkPublic,
    token: &str,
    issuer: &str,
    user_sub: &str,
    application_id: Id,
    vault_id_hash: &str,
    local_public_key: &str,
    remote_public_key: &str,
    now: i64,
) -> ErrorResponseResult<()> {
    let (header, claims) = verify_jwt::<TunnelAuthorizationClaims>(jwk, token)?;
    if header.kid != jwk.kid
        || !claims.valid_for(
            issuer,
            user_sub,
            application_id,
            vault_id_hash,
            local_public_key,
            remote_public_key,
            now,
        )
    {
        return Err(ErrorResponse::new(ErrorCode::AccessDenied));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use base64::{Engine, engine::general_purpose::STANDARD_NO_PAD};
    use idp_model::{
        contract::{
            JwkPrivate, JwkPrivateParameters, JwsAlgorithm, KeyUse, TunnelAuthorizationClaims,
        },
        model::Id,
    };
    use k256::ecdsa::SigningKey;

    use super::{super::encode_jwt, verify_tunnel_authorization};

    fn jwk() -> JwkPrivate {
        let signing_key = SigningKey::from_slice(&[1; 32]).unwrap();
        let point = signing_key.verifying_key().to_encoded_point(false);
        JwkPrivate {
            r#use: KeyUse::Signature,
            kid: "key".into(),
            alg: JwsAlgorithm::EdDSA,
            params: JwkPrivateParameters::Ec {
                crv: "secp256k1".into(),
                x: STANDARD_NO_PAD.encode(point.x().unwrap()),
                y: STANDARD_NO_PAD.encode(point.y().unwrap()),
                d: STANDARD_NO_PAD.encode(signing_key.to_bytes()),
            },
        }
    }

    fn claims() -> TunnelAuthorizationClaims {
        TunnelAuthorizationClaims {
            iss: "https://lidp.example".into(),
            sub: "1".into(),
            application_id: Id::nil(),
            vault_id_hash: "vault".into(),
            local_public_key: "local".into(),
            remote_public_key: "remote".into(),
            exp: 101,
            iat: 99,
            nbf: 99,
        }
    }

    #[test]
    fn rejects_a_grant_for_another_peer() {
        let private = jwk();
        let token = encode_jwt(&private, &claims()).unwrap();
        assert!(
            verify_tunnel_authorization(
                &private.into(),
                &token,
                "https://lidp.example",
                "1",
                Id::nil(),
                "vault",
                "local",
                "remote",
                100,
            )
            .is_ok()
        );
        assert!(
            verify_tunnel_authorization(
                &jwk().into(),
                &token,
                "https://lidp.example",
                "1",
                Id::nil(),
                "vault",
                "local",
                "other",
                100,
            )
            .is_err()
        );
    }
}
