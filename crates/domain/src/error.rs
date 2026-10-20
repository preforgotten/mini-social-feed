use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum DomainError {
    #[error("content too large: {size} bytes, max {max}")]
    ContentTooLarge { size: usize, max: usize },

    #[error("username too long: {len} chars, max {max}")]
    UsernameTooLong { len: usize, max: usize },

    #[error("bio too long: {len} chars, max {max}")]
    BioTooLong { len: usize, max: usize },

    #[error("username cannot be empty")]
    UsernameEmpty,
}
