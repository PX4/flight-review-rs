pub mod api;
pub mod db;
pub mod extract;
pub mod geocode;
pub mod storage;

use axum::{
    extract::DefaultBodyLimit,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

/// Shared application state passed to all handlers via axum's State extractor.
pub struct AppState {
    pub db: Arc<dyn db::LogStore>,
    pub storage: Arc<storage::FileStorage>,
    /// Prefix where v1 .ulg files live in the same storage backend.
    /// E.g., `flight_review/log_files` for `s3://bucket/flight_review/log_files/<uuid>.ulg`.
    pub v1_ulg_prefix: Option<String>,
    /// Mapbox access token for reverse geocoding at upload time.
    pub mapbox_token: Option<String>,
    /// Shared HTTP client for outbound requests (geocoding, etc.).
    pub http_client: reqwest::Client,
}

/// Build the application router. Shared by the binary (`main.rs`) and the
/// integration tests so they can never drift out of sync.
pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(api::health::health))
        .route("/api/version", get(api::version::version))
        .route(
            "/api/upload",
            post(api::upload::upload).layer(DefaultBodyLimit::max(512 * 1024 * 1024)), // 512 MB
        )
        .route("/api/logs", get(api::logs::list_logs))
        .route("/api/logs/facets", get(api::logs::list_facets))
        .route("/api/stats", get(api::stats::get_stats))
        .route(
            "/api/logs/{id}",
            get(api::logs::get_log).delete(api::logs::delete_log),
        )
        .route("/api/logs/{id}/track", get(api::logs::get_track))
        .route(
            "/api/logs/{id}/data/{filename}",
            get(api::logs::get_log_file),
        )
        .layer(CorsLayer::permissive())
        .with_state(state)
}
