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
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListFilters {
    pub sys_name: Option<String>,
    pub ver_hw: Option<String>,
    pub search: Option<String>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
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
