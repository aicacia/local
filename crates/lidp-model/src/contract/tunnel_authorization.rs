#[cfg(not(feature = "std"))]
use alloc::string::String;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct TunnelAuthorizationRequest {
    pub vault_id_hash: String,
    pub local_public_key: String,
    pub remote_public_key: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct TunnelAuthorization {
    pub token: String,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TunnelAuthorizationClaims {
    pub iss: String,
    pub sub: String,
    pub aud: String,
    pub vault_id_hash: String,
    pub local_public_key: String,
    pub remote_public_key: String,
    pub exp: i64,
    pub iat: i64,
    pub nbf: i64,
}

impl TunnelAuthorizationClaims {
    pub fn valid_for(
        &self,
        issuer: &str,
        user_sub: &str,
        client_id: &str,
        vault_id_hash: &str,
        local_public_key: &str,
        remote_public_key: &str,
        now: i64,
    ) -> bool {
        self.iss == issuer
            && self.sub == user_sub
            && self.aud == client_id
            && self.vault_id_hash == vault_id_hash
            && self.local_public_key == local_public_key
            && self.remote_public_key == remote_public_key
            && self.nbf <= now
            && self.exp > now
    }
}

#[cfg(test)]
mod tests {
    use super::TunnelAuthorizationClaims;

    fn claims() -> TunnelAuthorizationClaims {
        TunnelAuthorizationClaims {
            iss: "https://lidp.example".into(),
            sub: "1".into(),
            aud: "password-manager".into(),
            vault_id_hash: "vault".into(),
            local_public_key: "local".into(),
            remote_public_key: "remote".into(),
            exp: 101,
            iat: 99,
            nbf: 99,
        }
    }

    #[test]
    fn requires_the_complete_tunnel_binding() {
        assert!(claims().valid_for(
            "https://lidp.example",
            "1",
            "password-manager",
            "vault",
            "local",
            "remote",
            100,
        ));
        assert!(!claims().valid_for(
            "https://lidp.example",
            "1",
            "password-manager",
            "other-vault",
            "local",
            "remote",
            100,
        ));
        assert!(!claims().valid_for(
            "https://lidp.example",
            "1",
            "password-manager",
            "vault",
            "local",
            "remote",
            101,
        ));
    }
}
