#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("idp repository error: {0}")]
    Idp(#[from] idp_service::repo::RepoError),
    #[error("management service error: {0}")]
    Management(#[from] management_service::ManagementError),
}

pub type BootstrapResult<T> = Result<T, BootstrapError>;
