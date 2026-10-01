#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use model::contract::AuthorizationDetail;
use serde::{Deserialize, Deserializer, Serialize, de::Error};

use super::SubjectTokenType;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
pub struct TokenExchangeGrantRequest {
    pub subject_token: String,
    pub subject_token_type: SubjectTokenType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_token_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_authorization_details",
        skip_serializing_if = "Option::is_none"
    )]
    pub authorization_details: Option<Vec<AuthorizationDetail>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_token_type: Option<String>,
}

fn deserialize_authorization_details<'de, D>(
    deserializer: D,
) -> Result<Option<Vec<AuthorizationDetail>>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Details {
        Json(String),
        Values(Vec<AuthorizationDetail>),
    }

    match Option::<Details>::deserialize(deserializer)? {
        Some(Details::Json(value)) => serde_json::from_str(&value)
            .map(Some)
            .map_err(D::Error::custom),
        Some(Details::Values(values)) => Ok(Some(values)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use model::contract::{AuthorizationDetail, StorageAuthorizationAction};

    use super::TokenExchangeGrantRequest;

    #[test]
    fn parses_json_encoded_authorization_details() {
        let request: TokenExchangeGrantRequest = serde_json::from_str(
            r#"{
                "subject_token":"subject",
                "subject_token_type":"urn:ietf:params:oauth:token-type:access_token",
                "authorization_details":"[{\"type\":\"storage\",\"actions\":[\"read\",\"write\"]}]"
            }"#,
        )
        .expect("token exchange authorization details parse");

        assert_eq!(
            request.authorization_details,
            Some(vec![AuthorizationDetail::Storage(
                model::contract::StorageAuthorizationDetail {
                    actions: vec![
                        StorageAuthorizationAction::Read,
                        StorageAuthorizationAction::Write
                    ],
                }
            )])
        );
    }
}
