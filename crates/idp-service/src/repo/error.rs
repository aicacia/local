use core::error::Error;

use idp_model::contract::{ErrorCode, ErrorResponse};
use key::DerivationPath;

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("deserialize error: {0}")]
    DeserializeError(#[from] serde::de::value::Error),

    #[error("key error: {0}")]
    Key(#[from] key::KeyError),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("no access to key: {0}")]
    NoAccessToKey(DerivationPath),
    #[error("{0}")]
    Other(#[from] Box<dyn Error + Send + Sync>),
}

impl RepoError {
    pub fn other<E>(error: E) -> Self
    where
        E: Into<Box<dyn Error + Send + Sync>>,
    {
        RepoError::Other(error.into())
    }
}

impl From<keyring_core::Error> for RepoError {
    fn from(err: keyring_core::Error) -> Self {
        Self::Other(Box::new(err))
    }
}

impl From<RepoError> for ErrorResponse {
    fn from(error: RepoError) -> Self {
        ErrorResponse::new(ErrorCode::ServerError).with_description(error.to_string())
    }
}

pub type RepoResult<T> = Result<T, RepoError>;
