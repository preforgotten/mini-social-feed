use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use mini_social_feed_app::AppError;
use mini_social_feed_data::DataError;
use mini_social_feed_domain::DomainError;
use serde_json::json;

#[derive(Debug)]
pub enum HttpError {
    BadRequest(String),
    Unauthorized(String),
    NotFound(String),
    Conflict(String),
    PayloadTooLarge(String),
    Internal(String),
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            HttpError::BadRequest(m) => (StatusCode::BAD_REQUEST, m.as_str()),
            HttpError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m.as_str()),
            HttpError::NotFound(m) => (StatusCode::NOT_FOUND, m.as_str()),
            HttpError::Conflict(m) => (StatusCode::CONFLICT, m.as_str()),
            HttpError::PayloadTooLarge(m) => (StatusCode::PAYLOAD_TOO_LARGE, m.as_str()),
            HttpError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m.as_str()),
        };
        (status, Json(json!({"error": msg}))).into_response()
    }
}

// -- AppError → HttpError ----------------------------------------------------

impl From<AppError> for HttpError {
    fn from(e: AppError) -> Self {
        match &e {
            AppError::NotFound(m) => HttpError::NotFound(m.clone()),
            AppError::Duplicate(m) => HttpError::Conflict(m.clone()),
            AppError::Unauthorized => HttpError::Unauthorized(e.to_string()),
            AppError::InvalidToken => HttpError::Unauthorized("invalid or expired token".into()),
            AppError::TokenFamilyRevoked => HttpError::Unauthorized(e.to_string()),
            AppError::BadRequest(m) => HttpError::BadRequest(m.clone()),
            AppError::Internal(m) => HttpError::Internal(m.clone()),
            AppError::Data(de) => de.clone().into(),
            AppError::Domain(de) => de.clone().into(),
        }
    }
}

// -- DataError → HttpError ---------------------------------------------------

impl From<DataError> for HttpError {
    fn from(e: DataError) -> Self {
        match &e {
            DataError::NotFound(m) => HttpError::NotFound(m.clone()),
            DataError::Duplicate(m) => HttpError::Conflict(m.clone()),
            DataError::BadRequest(m) => HttpError::BadRequest(m.clone()),
            DataError::Internal(m) => HttpError::Internal(m.clone()),
            DataError::Domain(de) => de.clone().into(),
        }
    }
}

// -- DomainError → HttpError -------------------------------------------------

impl From<DomainError> for HttpError {
    fn from(e: DomainError) -> Self {
        match &e {
            DomainError::ContentTooLarge { .. } => HttpError::BadRequest(e.to_string()),
            DomainError::UsernameTooLong { .. } => HttpError::BadRequest(e.to_string()),
            DomainError::BioTooLong { .. } => HttpError::BadRequest(e.to_string()),
            DomainError::UsernameEmpty => HttpError::BadRequest(e.to_string()),
        }
    }
}
