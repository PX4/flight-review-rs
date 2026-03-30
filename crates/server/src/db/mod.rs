pub mod postgres;
pub mod sqlite;

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// QueryBuilder – abstracts `?` (SQLite) vs `$N` (Postgres) parameter style
// ---------------------------------------------------------------------------

/// Parameter placeholder style.
#[derive(Debug, Clone, Copy)]
pub enum ParamStyle {
    /// SQLite-style `?` placeholders.
    Placeholder,
    /// Postgres-style `$1, $2, …` positional placeholders.
    Positional,
}

/// A value that can be bound to a query parameter.
#[derive(Debug, Clone)]
pub enum BindValue {
    Str(String),
    OptStr(Option<String>),
    Int(i64),
    Float(f64),
    Bool(bool),
}

/// Builds a dynamic WHERE clause with correctly-numbered placeholders.
#[derive(Debug)]
pub struct QueryBuilder {
    conditions: Vec<String>,
    bind_values: Vec<BindValue>,
    style: ParamStyle,
    param_counter: usize,
}

impl QueryBuilder {
    pub fn new(style: ParamStyle) -> Self {
        Self {
            conditions: Vec::new(),
            bind_values: Vec::new(),
            style,
            param_counter: 0,
        }
    }

    /// Next placeholder string: `?` for SQLite, `$N` for Postgres.
    pub fn next_param(&mut self) -> String {
        self.param_counter += 1;
        match self.style {
            ParamStyle::Placeholder => "?".to_string(),
            ParamStyle::Positional => format!("${}", self.param_counter),
        }
    }

    /// Add exact-match condition: `column = <placeholder>`.
    pub fn add_eq(&mut self, column: &str, value: BindValue) {
        let p = self.next_param();
        self.conditions.push(format!("{column} = {p}"));
        self.bind_values.push(value);
    }

    /// Add LIKE condition: `column LIKE <placeholder>`.
    pub fn add_like(&mut self, column: &str, pattern: String) {
        let p = self.next_param();
        self.conditions.push(format!("{column} LIKE {p}"));
        self.bind_values.push(BindValue::Str(pattern));
    }

    /// Add ILIKE condition (Postgres): `column ILIKE <placeholder>`.
    pub fn add_ilike(&mut self, column: &str, pattern: String) {
        let p = self.next_param();
        self.conditions.push(format!("{column} ILIKE {p}"));
        self.bind_values.push(BindValue::Str(pattern));
    }

    /// Add `>=` condition.
    pub fn add_gte(&mut self, column: &str, value: BindValue) {
        let p = self.next_param();
        self.conditions.push(format!("{column} >= {p}"));
        self.bind_values.push(value);
    }

    /// Add `<=` condition.
    pub fn add_lte(&mut self, column: &str, value: BindValue) {
        let p = self.next_param();
        self.conditions.push(format!("{column} <= {p}"));
        self.bind_values.push(value);
    }

    /// Add `IS NULL` or `IS NOT NULL` condition (no bind value).
    pub fn add_null(&mut self, column: &str, should_be_null: bool) {
        if should_be_null {
            self.conditions.push(format!("{column} IS NULL"));
        } else {
            self.conditions.push(format!("{column} IS NOT NULL"));
        }
    }

    /// Add a raw SQL condition with no bind value.
    pub fn add_raw(&mut self, condition: String) {
        self.conditions.push(condition);
    }

    /// Build the `WHERE …` clause. Returns an empty string if there are no conditions.
    pub fn where_clause(&self) -> String {
        if self.conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", self.conditions.join(" AND "))
        }
    }

    /// Ordered bind values ready for binding.
    pub fn values(&self) -> &[BindValue] {
        &self.bind_values
    }
}

// ---------------------------------------------------------------------------
// LogRecord
// ---------------------------------------------------------------------------

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

    // ---- search / analytics columns (populated by extract_search_fields) ----
    /// System UUID from parameters
    pub sys_uuid: Option<String>,
    /// Software version tag (e.g. "v1.14.3")
    pub ver_sw: Option<String>,
    /// Vehicle type: "multirotor", "fixedwing", "vtol", "rover", "other"
    pub vehicle_type: Option<String>,
    /// Comma-separated localization sources (e.g. "gps,optical_flow,vision")
    pub localization_sources: Option<String>,
    /// Vibration quality: "good", "warning", "critical"
    pub vibration_status: Option<String>,
    /// Minimum battery cell voltage (V)
    pub battery_min_voltage: Option<f64>,
    /// Maximum GPS horizontal position error (m)
    pub gps_max_eph: Option<f64>,
    /// Maximum ground speed (m/s)
    pub max_speed_m_s: Option<f64>,
    /// Total distance travelled (m)
    pub total_distance_m: Option<f64>,
    /// Number of error-level events/messages
    pub error_count: Option<i32>,
    /// Number of warning-level events/messages
    pub warning_count: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListFilters {
    // Existing filters
    pub sys_name: Option<String>,
    pub ver_hw: Option<String>,
    pub search: Option<String>,
    pub offset: Option<i64>,
    pub limit: Option<i64>,
    /// If true, include private logs in results (default: only public)
    pub include_private: Option<bool>,

    // Phase 1 search filters
    /// ISO-8601 date string lower bound on created_at
    pub date_from: Option<String>,
    /// ISO-8601 date string upper bound on created_at
    pub date_to: Option<String>,
    /// Minimum flight duration in seconds
    pub flight_duration_min: Option<f64>,
    /// Maximum flight duration in seconds
    pub flight_duration_max: Option<f64>,
    /// Filter by software release version string (prefix match)
    pub ver_sw_release_str: Option<String>,
    /// Filter by software git hash (exact match)
    pub ver_sw: Option<String>,
    /// Filter by system UUID (exact match)
    pub sys_uuid: Option<String>,
    /// Filter by vehicle type category (exact match)
    pub vehicle_type: Option<String>,
    /// Filter by localization source (substring match)
    pub localization: Option<String>,
    /// Filter by vibration status (exact match)
    pub vibration_status: Option<String>,
    /// Filter by GPS presence (lat IS NOT NULL)
    pub has_gps: Option<bool>,
    /// Sort column and direction, e.g. "created_at:desc", "flight_duration_s:asc"
    pub sort: Option<String>,

    // Junction table / geo filters
    /// Filter logs that contain this topic name
    pub has_topic: Option<String>,
    /// Filter by parameter "name:value" (e.g. "EKF2_AID_MASK:24")
    pub parameter: Option<String>,
    /// Filter by tag
    pub tag: Option<String>,
    /// Filter by error message substring
    pub error_message: Option<String>,
    /// Center latitude for geographic search
    pub lat: Option<f64>,
    /// Center longitude for geographic search
    pub lon: Option<f64>,
    /// Radius in km for geographic search
    pub radius_km: Option<f64>,

    /// Filter: field max exceeds threshold. Format: "topic.field:value"
    pub field_max: Option<String>,
    /// Filter: field min below threshold. Format: "topic.field:value"
    pub field_min: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListResponse {
    pub logs: Vec<LogRecord>,
    pub total: i64,
}

// ---------------------------------------------------------------------------
// StatsParams / StatRow – used by the aggregation endpoint
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct StatsParams {
    /// Column to group by (validated against allowlist in the handler).
    pub group_by: String,
    /// Time period: "7d", "30d", "90d", "1y", "all".
    pub period: Option<String>,
    /// Maximum number of groups to return.
    pub limit: Option<i64>,
    // Optional filters (subset of ListFilters).
    pub vehicle_type: Option<String>,
    pub ver_hw: Option<String>,
    pub ver_sw_release_str: Option<String>,
    pub source: Option<String>,
    pub vibration_status: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StatRow {
    pub group: String,
    pub count: i64,
    pub avg_flight_duration_s: Option<f64>,
    pub total_flight_hours: Option<f64>,
    pub avg_max_speed: Option<f64>,
}

/// Parse a period string into a number of days. Returns `None` for "all".
pub fn period_to_days(period: Option<&str>) -> Option<i64> {
    match period {
        None => Some(30),
        Some("all") => None,
        Some("7d") => Some(7),
        Some("30d") => Some(30),
        Some("90d") => Some(90),
        Some("1y") => Some(365),
        Some(_) => Some(30),
    }
}

#[async_trait::async_trait]
pub trait LogStore: Send + Sync {
    async fn insert(&self, record: &LogRecord) -> Result<(), DbError>;
    async fn get(&self, id: Uuid) -> Result<Option<LogRecord>, DbError>;
    async fn list(&self, filters: &ListFilters) -> Result<ListResponse, DbError>;
    async fn delete(&self, id: Uuid) -> Result<bool, DbError>;
    async fn update(&self, id: Uuid, record: &LogRecord) -> Result<(), DbError>;

    // Aggregation
    async fn stats(&self, params: &StatsParams) -> Result<Vec<StatRow>, DbError>;

    // Junction table methods
    async fn insert_parameters(&self, log_id: Uuid, params: &[(String, f64)]) -> Result<(), DbError>;
    async fn insert_topics(&self, log_id: Uuid, topics: &[(String, i32)]) -> Result<(), DbError>;
    async fn insert_tags(&self, log_id: Uuid, tags: &[String]) -> Result<(), DbError>;
    async fn insert_errors(&self, log_id: Uuid, errors: &[(String, String, Option<u64>)]) -> Result<(), DbError>;
    async fn insert_field_stats(&self, log_id: Uuid, stats: &[FieldStatRecord]) -> Result<(), DbError>;
    async fn delete_junction_data(&self, log_id: Uuid) -> Result<(), DbError>;
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

/// Compute a lat/lon bounding box from a center point and radius in km.
/// Returns (min_lat, max_lat, min_lon, max_lon).
pub fn bounding_box(lat: f64, lon: f64, radius_km: f64) -> (f64, f64, f64, f64) {
    let lat_delta = radius_km / 111.32; // 1 degree lat ~ 111.32 km
    let cos_lat = lat.to_radians().cos();
    let lon_delta = if cos_lat.abs() < 1e-10 {
        180.0 // near poles, use full longitude range
    } else {
        radius_km / (111.32 * cos_lat)
    };
    (lat - lat_delta, lat + lat_delta, lon - lon_delta, lon + lon_delta)
}

/// A record for the `log_field_stats` junction table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldStatRecord {
    pub topic: String,
    pub field: String,
    pub min_val: f64,
    pub max_val: f64,
    pub mean_val: f64,
    pub count: i64,
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
