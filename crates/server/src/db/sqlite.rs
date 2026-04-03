#[cfg(feature = "sqlite")]
use super::{period_to_days, DbError, FacetsResponse, ListFilters, ListResponse, LogRecord, LogStore, StatRow, StatsParams};
#[cfg(feature = "sqlite")]
use chrono::{DateTime, Utc};
#[cfg(feature = "sqlite")]
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
#[cfg(feature = "sqlite")]
use sqlx::{Row, SqlitePool};
#[cfg(feature = "sqlite")]
use std::str::FromStr;
#[cfg(feature = "sqlite")]
use uuid::Uuid;

#[cfg(feature = "sqlite")]
const CREATE_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS logs (
    id TEXT PRIMARY KEY NOT NULL,
    filename TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    file_size INTEGER NOT NULL DEFAULT 0,
    sys_name TEXT,
    ver_hw TEXT,
    ver_sw_release_str TEXT,
    flight_duration_s REAL,
    topic_count INTEGER NOT NULL DEFAULT 0,
    lat REAL,
    lon REAL,
    is_public INTEGER NOT NULL DEFAULT 0,
    delete_token TEXT NOT NULL DEFAULT '',
    description TEXT,
    wind_speed TEXT,
    rating INTEGER,
    feedback TEXT,
    video_url TEXT,
    source TEXT,
    pilot_name TEXT,
    vehicle_name TEXT,
    tags TEXT,
    location_name TEXT,
    mission_type TEXT,
    sys_uuid TEXT,
    ver_sw TEXT,
    vehicle_type TEXT,
    localization_sources TEXT,
    vibration_status TEXT,
    battery_min_voltage REAL,
    gps_max_eph REAL,
    max_speed_m_s REAL,
    total_distance_m REAL,
    error_count INTEGER,
    warning_count INTEGER
);
CREATE INDEX IF NOT EXISTS idx_logs_created_at ON logs(created_at);
CREATE INDEX IF NOT EXISTS idx_logs_sys_name ON logs(sys_name);
CREATE INDEX IF NOT EXISTS idx_logs_ver_hw ON logs(ver_hw);
CREATE INDEX IF NOT EXISTS idx_logs_sys_uuid ON logs(sys_uuid);
CREATE INDEX IF NOT EXISTS idx_logs_vehicle_type ON logs(vehicle_type);
CREATE INDEX IF NOT EXISTS idx_logs_vibration_status ON logs(vibration_status);
CREATE INDEX IF NOT EXISTS idx_logs_ver_sw ON logs(ver_sw);
CREATE INDEX IF NOT EXISTS idx_logs_flight_duration ON logs(flight_duration_s);
CREATE INDEX IF NOT EXISTS idx_logs_lat_lon ON logs(lat, lon);
"#;

#[cfg(feature = "sqlite")]
const CREATE_JUNCTION_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS log_parameters (
    log_id TEXT NOT NULL,
    name TEXT NOT NULL,
    value REAL NOT NULL,
    PRIMARY KEY (log_id, name),
    FOREIGN KEY (log_id) REFERENCES logs(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_log_params_name ON log_parameters(name);
CREATE INDEX IF NOT EXISTS idx_log_params_name_val ON log_parameters(name, value);

CREATE TABLE IF NOT EXISTS log_topics (
    log_id TEXT NOT NULL,
    topic_name TEXT NOT NULL,
    message_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (log_id, topic_name),
    FOREIGN KEY (log_id) REFERENCES logs(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_log_topics_topic ON log_topics(topic_name);

CREATE TABLE IF NOT EXISTS log_tags (
    log_id TEXT NOT NULL,
    tag TEXT NOT NULL,
    PRIMARY KEY (log_id, tag),
    FOREIGN KEY (log_id) REFERENCES logs(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_log_tags_tag ON log_tags(tag);

CREATE TABLE IF NOT EXISTS log_errors (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    log_id TEXT NOT NULL,
    level TEXT NOT NULL,
    message TEXT NOT NULL,
    timestamp_us INTEGER,
    FOREIGN KEY (log_id) REFERENCES logs(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_log_errors_log ON log_errors(log_id);
CREATE INDEX IF NOT EXISTS idx_log_errors_level ON log_errors(level);

CREATE TABLE IF NOT EXISTS log_field_stats (
    log_id TEXT NOT NULL,
    topic TEXT NOT NULL,
    field TEXT NOT NULL,
    min_val REAL,
    max_val REAL,
    mean_val REAL,
    count INTEGER,
    PRIMARY KEY (log_id, topic, field),
    FOREIGN KEY (log_id) REFERENCES logs(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_field_stats_topic_field ON log_field_stats(topic, field);
CREATE INDEX IF NOT EXISTS idx_field_stats_topic_field_max ON log_field_stats(topic, field, max_val);
CREATE INDEX IF NOT EXISTS idx_field_stats_topic_field_min ON log_field_stats(topic, field, min_val);
"#;

#[cfg(feature = "sqlite")]
const ALTER_COLUMNS: &[&str] = &[
    "ALTER TABLE logs ADD COLUMN sys_uuid TEXT",
    "ALTER TABLE logs ADD COLUMN ver_sw TEXT",
    "ALTER TABLE logs ADD COLUMN vehicle_type TEXT",
    "ALTER TABLE logs ADD COLUMN localization_sources TEXT",
    "ALTER TABLE logs ADD COLUMN vibration_status TEXT",
    "ALTER TABLE logs ADD COLUMN battery_min_voltage REAL",
    "ALTER TABLE logs ADD COLUMN gps_max_eph REAL",
    "ALTER TABLE logs ADD COLUMN max_speed_m_s REAL",
    "ALTER TABLE logs ADD COLUMN total_distance_m REAL",
    "ALTER TABLE logs ADD COLUMN error_count INTEGER",
    "ALTER TABLE logs ADD COLUMN warning_count INTEGER",
];

#[cfg(feature = "sqlite")]
pub struct SqliteStore {
    pool: SqlitePool,
}

#[cfg(not(feature = "sqlite"))]
pub struct SqliteStore {
    _phantom: std::marker::PhantomData<()>,
}

#[cfg(feature = "sqlite")]
impl SqliteStore {
    pub async fn new(url: &str) -> Result<Self, super::DbError> {
        let opts = SqliteConnectOptions::from_str(url)?.create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;

        // Enable foreign key support for cascade deletes.
        sqlx::query("PRAGMA foreign_keys = ON").execute(&pool).await?;

        sqlx::query(CREATE_TABLE).execute(&pool).await?;

        // Create junction tables for parameters, topics, tags, errors.
        sqlx::query(CREATE_JUNCTION_TABLES).execute(&pool).await?;

        // Migrate: add new columns to existing tables (idempotent).
        for col_sql in ALTER_COLUMNS {
            match sqlx::query(col_sql).execute(&pool).await {
                Ok(_) => {}
                Err(e) if e.to_string().contains("duplicate column") => {}
                Err(e) => return Err(e.into()),
            }
        }

        // Create indexes that were added after the initial schema.
        for idx_sql in &[
            "CREATE INDEX IF NOT EXISTS idx_logs_sys_uuid ON logs(sys_uuid)",
            "CREATE INDEX IF NOT EXISTS idx_logs_vehicle_type ON logs(vehicle_type)",
            "CREATE INDEX IF NOT EXISTS idx_logs_vibration_status ON logs(vibration_status)",
            "CREATE INDEX IF NOT EXISTS idx_logs_ver_sw ON logs(ver_sw)",
            "CREATE INDEX IF NOT EXISTS idx_logs_flight_duration ON logs(flight_duration_s)",
            "CREATE INDEX IF NOT EXISTS idx_logs_lat_lon ON logs(lat, lon)",
        ] {
            sqlx::query(idx_sql).execute(&pool).await?;
        }

        Ok(Self { pool })
    }
}

#[cfg(not(feature = "sqlite"))]
impl SqliteStore {
    pub async fn new(_url: &str) -> Result<Self, super::DbError> {
        Err(super::DbError::Sqlx(sqlx::Error::Configuration(
            "sqlite feature is not enabled".into(),
        )))
    }
}

#[cfg(feature = "sqlite")]
fn row_to_record(row: &sqlx::sqlite::SqliteRow) -> Result<LogRecord, sqlx::Error> {
    let id_str: String = row.try_get("id")?;
    let id = Uuid::parse_str(&id_str).map_err(|e| sqlx::Error::ColumnDecode {
        index: "id".to_string(),
        source: Box::new(e),
    })?;

    let created_str: String = row.try_get("created_at")?;
    let created_at = DateTime::parse_from_rfc3339(&created_str)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(&created_str, "%Y-%m-%d %H:%M:%S")
                .map(|ndt| ndt.and_utc())
        })
        .unwrap_or_default();

    let is_public_int: i32 = row.try_get("is_public")?;

    Ok(LogRecord {
        id,
        filename: row.try_get("filename")?,
        created_at,
        file_size: row.try_get("file_size")?,
        sys_name: row.try_get("sys_name")?,
        ver_hw: row.try_get("ver_hw")?,
        ver_sw_release_str: row.try_get("ver_sw_release_str")?,
        flight_duration_s: row.try_get("flight_duration_s")?,
        topic_count: row.try_get("topic_count")?,
        lat: row.try_get("lat")?,
        lon: row.try_get("lon")?,
        is_public: is_public_int != 0,
        delete_token: row.try_get("delete_token")?,
        description: row.try_get("description")?,
        wind_speed: row.try_get("wind_speed")?,
        rating: row.try_get("rating")?,
        feedback: row.try_get("feedback")?,
        video_url: row.try_get("video_url")?,
        source: row.try_get("source")?,
        pilot_name: row.try_get("pilot_name")?,
        vehicle_name: row.try_get("vehicle_name")?,
        tags: row.try_get("tags")?,
        location_name: row.try_get("location_name")?,
        mission_type: row.try_get("mission_type")?,
        sys_uuid: row.try_get("sys_uuid")?,
        ver_sw: row.try_get("ver_sw")?,
        vehicle_type: row.try_get("vehicle_type")?,
        localization_sources: row.try_get("localization_sources")?,
        vibration_status: row.try_get("vibration_status")?,
        battery_min_voltage: row.try_get("battery_min_voltage")?,
        gps_max_eph: row.try_get("gps_max_eph")?,
        max_speed_m_s: row.try_get("max_speed_m_s")?,
        total_distance_m: row.try_get("total_distance_m")?,
        error_count: row.try_get("error_count")?,
        warning_count: row.try_get("warning_count")?,
    })
}

/// Parse a sort specification like "created_at:desc" into a safe ORDER BY clause.
#[cfg(feature = "sqlite")]
fn parse_sort(sort: Option<&str>) -> String {
    const ALLOWED: &[&str] = &[
        "created_at",
        "flight_duration_s",
        "file_size",
        "max_speed_m_s",
        "total_distance_m",
        "battery_min_voltage",
    ];
    match sort {
        Some(s) => {
            let mut parts = s.splitn(2, ':');
            let col = parts.next().unwrap_or("created_at");
            let dir = parts.next().unwrap_or("desc");
            if !ALLOWED.contains(&col) {
                return "created_at DESC".to_string();
            }
            let dir = if dir == "asc" { "ASC" } else { "DESC" };
            format!("{} {}", col, dir)
        }
        None => "created_at DESC".to_string(),
    }
}

/// Build a WHERE clause and bind values from ListFilters for SQLite.
#[cfg(feature = "sqlite")]
fn build_where_sqlite(filters: &ListFilters) -> (String, Vec<String>) {
    let mut conditions = Vec::new();
    let mut bind_values: Vec<String> = Vec::new();

    if !filters.include_private.unwrap_or(false) {
        conditions.push("is_public = 1".to_string());
    }
    if let Some(ref sys_name) = filters.sys_name {
        conditions.push("sys_name = ?".to_string());
        bind_values.push(sys_name.clone());
    }
    if let Some(ref ver_hw) = filters.ver_hw {
        conditions.push("ver_hw = ?".to_string());
        bind_values.push(ver_hw.clone());
    }
    if let Some(ref search) = filters.search {
        conditions.push(
            "(filename LIKE ? OR sys_name LIKE ? OR ver_hw LIKE ? OR description LIKE ? \
             OR vehicle_name LIKE ? OR tags LIKE ? OR location_name LIKE ? \
             OR ver_sw_release_str LIKE ? OR vehicle_type LIKE ?)"
                .to_string(),
        );
        let pattern = format!("%{}%", search);
        for _ in 0..9 {
            bind_values.push(pattern.clone());
        }
    }
    if let Some(ref date_from) = filters.date_from {
        conditions.push("created_at >= ?".to_string());
        bind_values.push(date_from.clone());
    }
    if let Some(ref date_to) = filters.date_to {
        conditions.push("created_at <= ?".to_string());
        bind_values.push(date_to.clone());
    }
    if let Some(min) = filters.flight_duration_min {
        conditions.push("flight_duration_s >= ?".to_string());
        bind_values.push(min.to_string());
    }
    if let Some(max) = filters.flight_duration_max {
        conditions.push("flight_duration_s <= ?".to_string());
        bind_values.push(max.to_string());
    }
    if let Some(ref v) = filters.ver_sw_release_str {
        conditions.push("ver_sw_release_str LIKE ?".to_string());
        bind_values.push(format!("{}%", v));
    }
    if let Some(ref v) = filters.ver_sw {
        conditions.push("ver_sw = ?".to_string());
        bind_values.push(v.clone());
    }
    if let Some(ref v) = filters.sys_uuid {
        conditions.push("sys_uuid = ?".to_string());
        bind_values.push(v.clone());
    }
    if let Some(ref v) = filters.vehicle_type {
        conditions.push("vehicle_type = ?".to_string());
        bind_values.push(v.clone());
    }
    if let Some(ref v) = filters.localization {
        conditions.push("localization_sources LIKE ?".to_string());
        bind_values.push(format!("%{}%", v));
    }
    if let Some(ref v) = filters.vibration_status {
        conditions.push("vibration_status = ?".to_string());
        bind_values.push(v.clone());
    }
    if let Some(has) = filters.has_gps {
        if has {
            conditions.push("lat IS NOT NULL".to_string());
        } else {
            conditions.push("lat IS NULL".to_string());
        }
    }
    if let Some(ref v) = filters.location_name {
        conditions.push("location_name LIKE ?".to_string());
        bind_values.push(format!("%{}%", v));
    }
    if let Some(ref topic) = filters.has_topic {
        conditions.push(
            "EXISTS (SELECT 1 FROM log_topics WHERE log_id = logs.id AND topic_name = ?)".to_string(),
        );
        bind_values.push(topic.clone());
    }
    if let Some(ref param_str) = filters.parameter {
        if let Some((name, value_str)) = param_str.split_once(':') {
            if let Ok(value) = value_str.parse::<f64>() {
                conditions.push(
                    "EXISTS (SELECT 1 FROM log_parameters WHERE log_id = logs.id AND name = ? AND value = ?)".to_string(),
                );
                bind_values.push(name.to_string());
                bind_values.push(value.to_string());
            }
        }
    }
    if let Some(ref tag) = filters.tag {
        conditions.push(
            "EXISTS (SELECT 1 FROM log_tags WHERE log_id = logs.id AND tag = ?)".to_string(),
        );
        bind_values.push(tag.clone());
    }
    if let Some(ref err_msg) = filters.error_message {
        conditions.push(
            "EXISTS (SELECT 1 FROM log_errors WHERE log_id = logs.id AND message LIKE ?)".to_string(),
        );
        bind_values.push(format!("%{}%", err_msg));
    }
    if let Some(ref fm) = filters.field_max {
        if let Some((topic_field, val_str)) = fm.split_once(':') {
            if let Some((topic, field)) = topic_field.split_once('.') {
                if let Ok(val) = val_str.parse::<f64>() {
                    conditions.push(
                        "EXISTS (SELECT 1 FROM log_field_stats WHERE log_id = logs.id AND topic = ? AND field = ? AND max_val >= ?)".to_string(),
                    );
                    bind_values.push(topic.to_string());
                    bind_values.push(field.to_string());
                    bind_values.push(val.to_string());
                }
            }
        }
    }
    if let Some(ref fm) = filters.field_min {
        if let Some((topic_field, val_str)) = fm.split_once(':') {
            if let Some((topic, field)) = topic_field.split_once('.') {
                if let Ok(val) = val_str.parse::<f64>() {
                    conditions.push(
                        "EXISTS (SELECT 1 FROM log_field_stats WHERE log_id = logs.id AND topic = ? AND field = ? AND min_val <= ?)".to_string(),
                    );
                    bind_values.push(topic.to_string());
                    bind_values.push(field.to_string());
                    bind_values.push(val.to_string());
                }
            }
        }
    }
    if let (Some(lat), Some(lon), Some(radius)) = (filters.lat, filters.lon, filters.radius_km) {
        let (min_lat, max_lat, min_lon, max_lon) = super::bounding_box(lat, lon, radius);
        conditions.push("lat BETWEEN ? AND ?".to_string());
        bind_values.push(min_lat.to_string());
        bind_values.push(max_lat.to_string());
        conditions.push("lon BETWEEN ? AND ?".to_string());
        bind_values.push(min_lon.to_string());
        bind_values.push(max_lon.to_string());
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    (where_clause, bind_values)
}

#[cfg(feature = "sqlite")]
#[async_trait::async_trait]
impl LogStore for SqliteStore {
    async fn insert(&self, record: &LogRecord) -> Result<(), DbError> {
        let id = record.id.to_string();
        let created_at = record.created_at.to_rfc3339();

        sqlx::query(
            "INSERT INTO logs (id, filename, created_at, file_size, sys_name, ver_hw, \
             ver_sw_release_str, flight_duration_s, topic_count, lat, lon, is_public, delete_token, \
             description, wind_speed, rating, feedback, video_url, source, pilot_name, \
             vehicle_name, tags, location_name, mission_type, \
             sys_uuid, ver_sw, vehicle_type, localization_sources, vibration_status, \
             battery_min_voltage, gps_max_eph, max_speed_m_s, total_distance_m, \
             error_count, warning_count) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, \
             ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&record.filename)
        .bind(&created_at)
        .bind(record.file_size)
        .bind(&record.sys_name)
        .bind(&record.ver_hw)
        .bind(&record.ver_sw_release_str)
        .bind(record.flight_duration_s)
        .bind(record.topic_count)
        .bind(record.lat)
        .bind(record.lon)
        .bind(record.is_public as i32)
        .bind(&record.delete_token)
        .bind(&record.description)
        .bind(&record.wind_speed)
        .bind(record.rating)
        .bind(&record.feedback)
        .bind(&record.video_url)
        .bind(&record.source)
        .bind(&record.pilot_name)
        .bind(&record.vehicle_name)
        .bind(&record.tags)
        .bind(&record.location_name)
        .bind(&record.mission_type)
        .bind(&record.sys_uuid)
        .bind(&record.ver_sw)
        .bind(&record.vehicle_type)
        .bind(&record.localization_sources)
        .bind(&record.vibration_status)
        .bind(record.battery_min_voltage)
        .bind(record.gps_max_eph)
        .bind(record.max_speed_m_s)
        .bind(record.total_distance_m)
        .bind(record.error_count)
        .bind(record.warning_count)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<LogRecord>, DbError> {
        let id_str = id.to_string();
        let row = sqlx::query("SELECT * FROM logs WHERE id = ?")
            .bind(&id_str)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(r) => Ok(Some(row_to_record(&r)?)),
            None => Ok(None),
        }
    }

    async fn list(&self, filters: &ListFilters) -> Result<ListResponse, DbError> {
        let (where_clause, bind_values) = build_where_sqlite(filters);

        // Sort
        let order_by = parse_sort(filters.sort.as_deref());

        // Count query
        let count_sql = format!("SELECT COUNT(*) as cnt FROM logs {}", where_clause);
        let mut count_query = sqlx::query(&count_sql);
        for v in &bind_values {
            count_query = count_query.bind(v);
        }
        let count_row = count_query.fetch_one(&self.pool).await?;
        let total: i64 = count_row.try_get("cnt")?;

        // Data query
        let limit = filters.limit.unwrap_or(50);
        let offset = filters.offset.unwrap_or(0);
        let data_sql = format!(
            "SELECT * FROM logs {} ORDER BY {} LIMIT ? OFFSET ?",
            where_clause, order_by
        );
        let mut data_query = sqlx::query(&data_sql);
        for v in &bind_values {
            data_query = data_query.bind(v);
        }
        data_query = data_query.bind(limit).bind(offset);

        let rows = data_query.fetch_all(&self.pool).await?;
        let logs: Result<Vec<LogRecord>, sqlx::Error> =
            rows.iter().map(row_to_record).collect();

        Ok(ListResponse {
            logs: logs?,
            total,
        })
    }

    async fn facets(&self, filters: &ListFilters) -> Result<FacetsResponse, DbError> {
        let (where_clause, bind_values) = build_where_sqlite(filters);

        let columns = ["ver_hw", "vehicle_type", "ver_sw_release_str", "vibration_status"];
        let mut results: Vec<Vec<String>> = Vec::new();

        for col in &columns {
            let sql = if where_clause.is_empty() {
                format!("SELECT DISTINCT {col} FROM logs WHERE {col} IS NOT NULL ORDER BY {col}")
            } else {
                format!("SELECT DISTINCT {col} FROM logs {where_clause} AND {col} IS NOT NULL ORDER BY {col}")
            };
            let mut query = sqlx::query(&sql);
            for v in &bind_values {
                query = query.bind(v);
            }
            let rows = query.fetch_all(&self.pool).await?;
            let vals: Vec<String> = rows.iter().filter_map(|r| r.try_get::<String, _>(col).ok()).collect();
            results.push(vals);
        }

        // Tags: split comma-separated values and deduplicate
        let tags_sql = if where_clause.is_empty() {
            "SELECT DISTINCT tags FROM logs WHERE tags IS NOT NULL AND tags != ''".to_string()
        } else {
            format!("SELECT DISTINCT tags FROM logs {where_clause} AND tags IS NOT NULL AND tags != ''")
        };
        let mut tags_query = sqlx::query(&tags_sql);
        for v in &bind_values {
            tags_query = tags_query.bind(v);
        }
        let tag_rows = tags_query.fetch_all(&self.pool).await?;
        let mut tag_set = std::collections::BTreeSet::new();
        for row in &tag_rows {
            if let Ok(tags_str) = row.try_get::<String, _>("tags") {
                for tag in tags_str.split(',') {
                    let t = tag.trim().to_string();
                    if !t.is_empty() {
                        tag_set.insert(t);
                    }
                }
            }
        }

        Ok(FacetsResponse {
            ver_hw: results.remove(0),
            vehicle_type: results.remove(0),
            ver_sw_release_str: results.remove(0),
            vibration_status: results.remove(0),
            tags: tag_set.into_iter().collect(),
        })
    }

    async fn delete(&self, id: Uuid) -> Result<bool, DbError> {
        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM logs WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn update(&self, id: Uuid, record: &LogRecord) -> Result<(), DbError> {
        let id_str = id.to_string();
        let created_at = record.created_at.to_rfc3339();

        sqlx::query(
            "UPDATE logs SET filename = ?, created_at = ?, file_size = ?, sys_name = ?, ver_hw = ?, \
             ver_sw_release_str = ?, flight_duration_s = ?, topic_count = ?, lat = ?, lon = ?, \
             is_public = ?, delete_token = ?, description = ?, wind_speed = ?, rating = ?, \
             feedback = ?, video_url = ?, source = ?, pilot_name = ?, vehicle_name = ?, \
             tags = ?, location_name = ?, mission_type = ?, \
             sys_uuid = ?, ver_sw = ?, vehicle_type = ?, localization_sources = ?, \
             vibration_status = ?, battery_min_voltage = ?, gps_max_eph = ?, \
             max_speed_m_s = ?, total_distance_m = ?, error_count = ?, warning_count = ? \
             WHERE id = ?",
        )
        .bind(&record.filename)
        .bind(&created_at)
        .bind(record.file_size)
        .bind(&record.sys_name)
        .bind(&record.ver_hw)
        .bind(&record.ver_sw_release_str)
        .bind(record.flight_duration_s)
        .bind(record.topic_count)
        .bind(record.lat)
        .bind(record.lon)
        .bind(record.is_public as i32)
        .bind(&record.delete_token)
        .bind(&record.description)
        .bind(&record.wind_speed)
        .bind(record.rating)
        .bind(&record.feedback)
        .bind(&record.video_url)
        .bind(&record.source)
        .bind(&record.pilot_name)
        .bind(&record.vehicle_name)
        .bind(&record.tags)
        .bind(&record.location_name)
        .bind(&record.mission_type)
        .bind(&record.sys_uuid)
        .bind(&record.ver_sw)
        .bind(&record.vehicle_type)
        .bind(&record.localization_sources)
        .bind(&record.vibration_status)
        .bind(record.battery_min_voltage)
        .bind(record.gps_max_eph)
        .bind(record.max_speed_m_s)
        .bind(record.total_distance_m)
        .bind(record.error_count)
        .bind(record.warning_count)
        .bind(&id_str)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn stats(&self, params: &StatsParams) -> Result<Vec<StatRow>, DbError> {
        let group_col = &params.group_by; // Already validated by handler

        // Build WHERE clause
        let mut conditions = vec!["is_public = 1".to_string()];
        let mut bind_values: Vec<String> = Vec::new();

        // Period filter
        if let Some(days) = period_to_days(params.period.as_deref()) {
            conditions.push(format!(
                "created_at >= datetime('now', '-{} days')",
                days
            ));
        }

        // Optional filters
        if let Some(ref v) = params.vehicle_type {
            conditions.push("vehicle_type = ?".to_string());
            bind_values.push(v.clone());
        }
        if let Some(ref v) = params.ver_hw {
            conditions.push("ver_hw = ?".to_string());
            bind_values.push(v.clone());
        }
        if let Some(ref v) = params.ver_sw_release_str {
            conditions.push("ver_sw_release_str = ?".to_string());
            bind_values.push(v.clone());
        }
        if let Some(ref v) = params.source {
            conditions.push("source = ?".to_string());
            bind_values.push(v.clone());
        }
        if let Some(ref v) = params.vibration_status {
            conditions.push("vibration_status = ?".to_string());
            bind_values.push(v.clone());
        }

        let where_clause = format!("WHERE {}", conditions.join(" AND "));
        let limit = params.limit.unwrap_or(50);

        let sql = format!(
            "SELECT {col} AS group_value, \
             COUNT(*) AS count, \
             AVG(flight_duration_s) AS avg_dur, \
             SUM(flight_duration_s) / 3600.0 AS total_hrs, \
             AVG(max_speed_m_s) AS avg_speed \
             FROM logs {where_clause} \
             AND {col} IS NOT NULL \
             GROUP BY {col} \
             ORDER BY count DESC \
             LIMIT ?",
            col = group_col,
            where_clause = where_clause
        );

        let mut query = sqlx::query(&sql);
        for v in &bind_values {
            query = query.bind(v);
        }
        query = query.bind(limit);

        let rows = query.fetch_all(&self.pool).await?;

        let data = rows
            .iter()
            .map(|row| StatRow {
                group: row.try_get::<String, _>("group_value").unwrap_or_default(),
                count: row.try_get("count").unwrap_or(0),
                avg_flight_duration_s: row.try_get("avg_dur").ok(),
                total_flight_hours: row.try_get("total_hrs").ok(),
                avg_max_speed: row.try_get("avg_speed").ok(),
            })
            .collect();

        Ok(data)
    }

    async fn insert_parameters(&self, log_id: Uuid, params: &[(String, f64)]) -> Result<(), DbError> {
        let id_str = log_id.to_string();
        for (name, value) in params {
            sqlx::query("INSERT OR REPLACE INTO log_parameters (log_id, name, value) VALUES (?, ?, ?)")
                .bind(&id_str)
                .bind(name)
                .bind(value)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    async fn insert_topics(&self, log_id: Uuid, topics: &[(String, i32)]) -> Result<(), DbError> {
        let id_str = log_id.to_string();
        for (topic_name, message_count) in topics {
            sqlx::query("INSERT OR REPLACE INTO log_topics (log_id, topic_name, message_count) VALUES (?, ?, ?)")
                .bind(&id_str)
                .bind(topic_name)
                .bind(message_count)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    async fn insert_tags(&self, log_id: Uuid, tags: &[String]) -> Result<(), DbError> {
        let id_str = log_id.to_string();
        for tag in tags {
            sqlx::query("INSERT OR REPLACE INTO log_tags (log_id, tag) VALUES (?, ?)")
                .bind(&id_str)
                .bind(tag)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    async fn insert_errors(&self, log_id: Uuid, errors: &[(String, String, Option<u64>)]) -> Result<(), DbError> {
        let id_str = log_id.to_string();
        for (level, message, timestamp_us) in errors {
            sqlx::query("INSERT INTO log_errors (log_id, level, message, timestamp_us) VALUES (?, ?, ?, ?)")
                .bind(&id_str)
                .bind(level)
                .bind(message)
                .bind(timestamp_us.map(|t| t as i64))
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    async fn insert_field_stats(&self, log_id: Uuid, stats: &[super::FieldStatRecord]) -> Result<(), DbError> {
        let id_str = log_id.to_string();
        for s in stats {
            sqlx::query(
                "INSERT OR REPLACE INTO log_field_stats (log_id, topic, field, min_val, max_val, mean_val, count) \
                 VALUES (?, ?, ?, ?, ?, ?, ?)"
            )
                .bind(&id_str)
                .bind(&s.topic)
                .bind(&s.field)
                .bind(s.min_val)
                .bind(s.max_val)
                .bind(s.mean_val)
                .bind(s.count)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    async fn delete_junction_data(&self, log_id: Uuid) -> Result<(), DbError> {
        let id_str = log_id.to_string();
        sqlx::query("DELETE FROM log_parameters WHERE log_id = ?").bind(&id_str).execute(&self.pool).await?;
        sqlx::query("DELETE FROM log_topics WHERE log_id = ?").bind(&id_str).execute(&self.pool).await?;
        sqlx::query("DELETE FROM log_tags WHERE log_id = ?").bind(&id_str).execute(&self.pool).await?;
        sqlx::query("DELETE FROM log_errors WHERE log_id = ?").bind(&id_str).execute(&self.pool).await?;
        sqlx::query("DELETE FROM log_field_stats WHERE log_id = ?").bind(&id_str).execute(&self.pool).await?;
        Ok(())
    }
}

#[cfg(not(feature = "sqlite"))]
#[async_trait::async_trait]
impl super::LogStore for SqliteStore {
    async fn insert(&self, _record: &super::LogRecord) -> Result<(), super::DbError> {
        Err(super::DbError::Sqlx(sqlx::Error::Configuration(
            "sqlite feature is not enabled".into(),
        )))
    }

    async fn get(&self, _id: uuid::Uuid) -> Result<Option<super::LogRecord>, super::DbError> {
        Err(super::DbError::Sqlx(sqlx::Error::Configuration(
            "sqlite feature is not enabled".into(),
        )))
    }

    async fn list(
        &self,
        _filters: &super::ListFilters,
    ) -> Result<super::ListResponse, super::DbError> {
        Err(super::DbError::Sqlx(sqlx::Error::Configuration(
            "sqlite feature is not enabled".into(),
        )))
    }

    async fn delete(&self, _id: uuid::Uuid) -> Result<bool, super::DbError> {
        Err(super::DbError::Sqlx(sqlx::Error::Configuration(
            "sqlite feature is not enabled".into(),
        )))
    }

    async fn update(&self, _id: uuid::Uuid, _record: &super::LogRecord) -> Result<(), super::DbError> {
        Err(super::DbError::Sqlx(sqlx::Error::Configuration(
            "sqlite feature is not enabled".into(),
        )))
    }

    async fn stats(&self, _params: &super::StatsParams) -> Result<Vec<super::StatRow>, super::DbError> {
        Err(super::DbError::Sqlx(sqlx::Error::Configuration(
            "sqlite feature is not enabled".into(),
        )))
    }

    async fn insert_parameters(&self, _log_id: uuid::Uuid, _params: &[(String, f64)]) -> Result<(), super::DbError> {
        Err(super::DbError::Sqlx(sqlx::Error::Configuration(
            "sqlite feature is not enabled".into(),
        )))
    }

    async fn insert_topics(&self, _log_id: uuid::Uuid, _topics: &[(String, i32)]) -> Result<(), super::DbError> {
        Err(super::DbError::Sqlx(sqlx::Error::Configuration(
            "sqlite feature is not enabled".into(),
        )))
    }

    async fn insert_tags(&self, _log_id: uuid::Uuid, _tags: &[String]) -> Result<(), super::DbError> {
        Err(super::DbError::Sqlx(sqlx::Error::Configuration(
            "sqlite feature is not enabled".into(),
        )))
    }

    async fn insert_errors(&self, _log_id: uuid::Uuid, _errors: &[(String, String, Option<u64>)]) -> Result<(), super::DbError> {
        Err(super::DbError::Sqlx(sqlx::Error::Configuration(
            "sqlite feature is not enabled".into(),
        )))
    }

    async fn insert_field_stats(&self, _log_id: uuid::Uuid, _stats: &[super::FieldStatRecord]) -> Result<(), super::DbError> {
        Err(super::DbError::Sqlx(sqlx::Error::Configuration(
            "sqlite feature is not enabled".into(),
        )))
    }

    async fn delete_junction_data(&self, _log_id: uuid::Uuid) -> Result<(), super::DbError> {
        Err(super::DbError::Sqlx(sqlx::Error::Configuration(
            "sqlite feature is not enabled".into(),
        )))
    }
}
