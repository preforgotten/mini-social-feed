use mini_social_feed_domain::DomainError;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum DataError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("duplicate: {0}")]
    Duplicate(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("internal: {0}")]
    Internal(String),

    #[error(transparent)]
    Domain(#[from] DomainError),
}
