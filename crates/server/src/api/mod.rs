pub mod health;
pub mod logs;
pub mod upload;

use axum::{http::StatusCode, response::IntoResponse, Json};

use crate::db::DbError;
use crate::storage::StorageError;

pub enum ApiError {
    NotFound,
    BadRequest(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not found".to_string()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::Internal(msg) => {
                tracing::error!("internal error: {msg}");
                (StatusCode::INTERNAL_SERVER_ERROR, msg)
            }
        };
        (status, Json(serde_json::json!({"error": message}))).into_response()
    }
}

impl From<DbError> for ApiError {
    fn from(err: DbError) -> Self {
        ApiError::Internal(err.to_string())
    }
}

impl From<StorageError> for ApiError {
    fn from(err: StorageError) -> Self {
        ApiError::Internal(err.to_string())
    }
}

impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        ApiError::Internal(err.to_string())
    }
}
