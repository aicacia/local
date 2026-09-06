use serde::{Deserialize, Deserializer, Serializer, de::Error};

use crate::contract::{
    ClientProfile, ClientType, CodeChallengeMethod, EntityType, Sex, TokenEndpointAuthMethod,
    UserDeviceState,
};

pub mod client_type {
    use super::*;

    pub fn serialize<S>(value: &ClientType, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(*value as i64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ClientType, D::Error>
    where
        D: Deserializer<'de>,
    {
        match i64::deserialize(deserializer)? {
            0 => Ok(ClientType::Confidential),
            1 => Ok(ClientType::Public),
            v => Err(D::Error::custom(format!("invalid ClientType value: {v}"))),
        }
    }
}

pub mod client_profile {
    use super::*;

    pub fn serialize<S>(value: &ClientProfile, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(*value as i64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<ClientProfile, D::Error>
    where
        D: Deserializer<'de>,
    {
        match i64::deserialize(deserializer)? {
            0 => Ok(ClientProfile::Web),
            1 => Ok(ClientProfile::UserAgentBased),
            2 => Ok(ClientProfile::Native),
            v => Err(D::Error::custom(format!(
                "invalid ClientProfile value: {v}"
            ))),
        }
    }
}

pub mod token_endpoint_auth_method {
    use super::*;

    pub fn serialize<S>(value: &TokenEndpointAuthMethod, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(*value as i64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<TokenEndpointAuthMethod, D::Error>
    where
        D: Deserializer<'de>,
    {
        match i64::deserialize(deserializer)? {
            0 => Ok(TokenEndpointAuthMethod::ClientSecretBasic),
            1 => Ok(TokenEndpointAuthMethod::ClientSecretPost),
            2 => Ok(TokenEndpointAuthMethod::PrivateKeyJwt),
            3 => Ok(TokenEndpointAuthMethod::ClientSecretJwt),
            4 => Ok(TokenEndpointAuthMethod::None),
            v => Err(D::Error::custom(format!(
                "invalid TokenEndpointAuthMethod value: {v}"
            ))),
        }
    }
}

pub mod code_challenge_method_option {
    use super::*;

    pub fn serialize<S>(
        value: &Option<CodeChallengeMethod>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(v) => serializer.serialize_some(&(*v as i64)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<CodeChallengeMethod>, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Option::<i64>::deserialize(deserializer)? {
            Some(0) => Ok(Some(CodeChallengeMethod::S256)),
            Some(v) => Err(D::Error::custom(format!(
                "invalid CodeChallengeMethod value: {v}"
            ))),
            None => Ok(None),
        }
    }
}

pub mod entity_type {
    use super::*;

    pub fn serialize<S>(value: &EntityType, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(*value as i64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<EntityType, D::Error>
    where
        D: Deserializer<'de>,
    {
        match i64::deserialize(deserializer)? {
            0 => Ok(EntityType::User),
            1 => Ok(EntityType::Client),
            v => Err(D::Error::custom(format!("invalid EntityType value: {v}"))),
        }
    }
}

pub mod user_device_state {
    use super::*;

    pub fn serialize<S>(value: &UserDeviceState, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(*value as i64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<UserDeviceState, D::Error>
    where
        D: Deserializer<'de>,
    {
        match i64::deserialize(deserializer)? {
            0 => Ok(UserDeviceState::Pending),
            1 => Ok(UserDeviceState::Approved),
            2 => Ok(UserDeviceState::Revoked),
            value => Err(D::Error::custom(format!(
                "invalid UserDeviceState value: {value}"
            ))),
        }
    }
}

pub mod sex_option {
    use super::*;

    pub fn serialize<S>(value: &Option<Sex>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match value {
            Some(v) => serializer.serialize_some(&(*v as i64)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Sex>, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Option::<i64>::deserialize(deserializer)? {
            Some(0) => Ok(Some(Sex::Male)),
            Some(1) => Ok(Some(Sex::Female)),
            Some(v) => Err(D::Error::custom(format!("invalid Sex value: {v}"))),
            None => Ok(None),
        }
    }
}
