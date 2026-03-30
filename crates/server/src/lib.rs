pub mod api;
pub mod db;
pub mod extract;
pub mod storage;

use std::sync::Arc;

/// Shared application state passed to all handlers via axum's State extractor.
pub struct AppState {
    pub db: Arc<dyn db::LogStore>,
    pub storage: Arc<storage::FileStorage>,
    /// Prefix where v1 .ulg files live in the same storage backend.
    /// E.g., `flight_review/log_files` for `s3://bucket/flight_review/log_files/<uuid>.ulg`.
    pub v1_ulg_prefix: Option<String>,
}
