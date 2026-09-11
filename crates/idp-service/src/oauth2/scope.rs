#[cfg(not(feature = "std"))]
use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};

use idp_model::contract::{ErrorCode, ErrorResponse, ErrorResponseResult};

pub fn parse_scopes(scope: &str) -> Vec<String> {
    scope
        .split_whitespace()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn scopes_to_string(scopes: &[String]) -> String {
    scopes.join(" ")
}

pub fn validate_scopes(requested: &[String], allowed: &[String]) -> ErrorResponseResult<()> {
    if requested.is_empty() {
        return Ok(());
    }

    if allowed.is_empty() {
        return Err(ErrorResponse::new(ErrorCode::InvalidScope)
            .with_description("client is not authorized for the requested scopes"));
    }

    let unauthorized = requested
        .iter()
        .filter(|scope| !allowed.contains(scope))
        .cloned()
        .collect::<Vec<_>>();

    if unauthorized.is_empty() {
        Ok(())
    } else {
        Err(ErrorResponse::new(ErrorCode::InvalidScope)
            .with_description(format!("unsupported scopes: {}", unauthorized.join(" "))))
    }
}

pub fn intersect_scopes(requested: &[String], allowed: &[String]) -> Vec<String> {
    if requested.is_empty() {
        return allowed.to_vec();
    }

    requested
        .iter()
        .filter(|scope| allowed.contains(scope))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scopes_splits_on_whitespace() {
        assert_eq!(
            parse_scopes("openid profile email"),
            vec!["openid", "profile", "email"]
        );
    }

    #[test]
    fn validate_scopes_rejects_unknown_scope() {
        let requested = vec!["openid".to_string(), "admin".to_string()];
        let allowed = vec!["openid".to_string(), "profile".to_string()];

        let error = validate_scopes(&requested, &allowed).unwrap_err();
        assert_eq!(error.error, ErrorCode::InvalidScope);
    }
}
