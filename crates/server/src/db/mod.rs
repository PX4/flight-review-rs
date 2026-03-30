pub mod postgres;
pub mod sqlite;

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRecord {
    pub id: Uuid,
    pub filename: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub file_size: i64,
    /// Vehicle name (from metadata sys_name)
    pub sys_name: Option<String>,
    /// Hardware version
    pub ver_hw: Option<String>,
    /// Software version string (human-readable)
    pub ver_sw_release_str: Option<String>,
    /// Flight duration in seconds
    pub flight_duration_s: Option<f64>,
    /// Number of topics
    pub topic_count: i32,
    /// GPS first-fix latitude
    pub lat: Option<f64>,
    /// GPS first-fix longitude
    pub lon: Option<f64>,
    /// Whether this log appears in public listings
    pub is_public: bool,
    /// Delete token (32-char hex). Required to delete a log.
    #[serde(skip_serializing)]
    pub delete_token: String,
    /// Free text description of the flight
    pub description: Option<String>,
    /// Wind conditions: "calm", "breeze", "gale", "storm"
    pub wind_speed: Option<String>,
    /// Flight quality rating 1-5
    pub rating: Option<i32>,
    /// Free text pilot feedback/notes
    pub feedback: Option<String>,
    /// Link to flight video
    pub video_url: Option<String>,
    /// Upload source: "web", "CI", "QGC", "API"
    pub source: Option<String>,
    /// Pilot name
    pub pilot_name: Option<String>,
    /// Vehicle name/callsign
    pub vehicle_name: Option<String>,
    /// Comma-separated tags
    pub tags: Option<String>,
    /// Human-readable location name
    pub location_name: Option<String>,
    /// Mission type: "survey", "inspection", "test", "recreational"
    pub mission_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListFilters {
    pub sys_name: Option<String>,
    pub ver_hw: Option<String>,
    pub search: Option<String>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
    /// If true, include private logs in results (default: only public)
    pub include_private: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListResponse {
    pub logs: Vec<LogRecord>,
    pub total: i64,
}

#[async_trait::async_trait]
pub trait LogStore: Send + Sync {
    async fn insert(&self, record: &LogRecord) -> Result<(), DbError>;
    async fn get(&self, id: Uuid) -> Result<Option<LogRecord>, DbError>;
    async fn list(&self, filters: &ListFilters) -> Result<ListResponse, DbError>;
    async fn delete(&self, id: Uuid) -> Result<bool, DbError>;
    async fn update(&self, id: Uuid, record: &LogRecord) -> Result<(), DbError>;
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

pub async fn create_db(url: &str) -> Result<Arc<dyn LogStore>, DbError> {
    if url.starts_with("sqlite:") {
        Ok(Arc::new(sqlite::SqliteStore::new(url).await?))
    } else if url.starts_with("postgres:") || url.starts_with("postgresql:") {
        Ok(Arc::new(postgres::PostgresStore::new(url).await?))
    } else {
        Err(DbError::Sqlx(sqlx::Error::Configuration(
            format!("unsupported database URL scheme: {}", url).into(),
        )))
    }
}
