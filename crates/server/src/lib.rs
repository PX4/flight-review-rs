pub mod api;
pub mod db;
pub mod storage;

use std::sync::Arc;

/// Shared application state passed to all handlers via axum's State extractor.
pub struct AppState {
    pub db: Arc<dyn db::LogStore>,
    pub storage: Arc<storage::FileStorage>,
}
