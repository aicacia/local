use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use idp_model::contract::{CodeChallengeMethod, ErrorCode, ErrorResponse, ErrorResponseResult};
use sha2::{Digest, Sha256};

pub fn verify_code_challenge(
    code_verifier: &str,
    code_challenge: &str,
    method: CodeChallengeMethod,
) -> ErrorResponseResult<()> {
    match method {
        CodeChallengeMethod::S256 => {
            let digest = Sha256::digest(code_verifier.as_bytes());
            let encoded = URL_SAFE_NO_PAD.encode(digest);

            if encoded == code_challenge {
                Ok(())
            } else {
                Err(ErrorResponse::new(ErrorCode::InvalidGrant))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_code_challenge_accepts_valid_s256_pair() {
        let code_verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let code_challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";

        verify_code_challenge(code_verifier, code_challenge, CodeChallengeMethod::S256).unwrap();
    }

    #[test]
    fn verify_code_challenge_rejects_invalid_pair() {
        let error = verify_code_challenge(
            "invalid-verifier",
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM",
            CodeChallengeMethod::S256,
        )
        .unwrap_err();

        assert_eq!(error.error, ErrorCode::InvalidGrant);
    }
}
