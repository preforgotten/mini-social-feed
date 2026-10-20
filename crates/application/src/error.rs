use mini_social_feed_domain::DomainError;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("duplicate: {0}")]
    Duplicate(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("invalid or expired token")]
    InvalidToken,

    #[error("token family revoked — full re-authentication required")]
    TokenFamilyRevoked,

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("internal: {0}")]
    Internal(String),

    #[error(transparent)]
    Data(#[from] mini_social_feed_data::DataError),

    #[error(transparent)]
    Domain(#[from] DomainError),
}
