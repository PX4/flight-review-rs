#[cfg(feature = "sqlite")]
use super::{DbError, ListFilters, ListResponse, LogRecord, LogStore};
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
    lon REAL
);
CREATE INDEX IF NOT EXISTS idx_logs_created_at ON logs(created_at);
CREATE INDEX IF NOT EXISTS idx_logs_sys_name ON logs(sys_name);
CREATE INDEX IF NOT EXISTS idx_logs_ver_hw ON logs(ver_hw);
"#;

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

        sqlx::query(CREATE_TABLE).execute(&pool).await?;

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
    })
}

#[cfg(feature = "sqlite")]
#[async_trait::async_trait]
impl LogStore for SqliteStore {
    async fn insert(&self, record: &LogRecord) -> Result<(), DbError> {
        let id = record.id.to_string();
        let created_at = record.created_at.to_rfc3339();

        sqlx::query(
            "INSERT INTO logs (id, filename, created_at, file_size, sys_name, ver_hw, \
             ver_sw_release_str, flight_duration_s, topic_count, lat, lon) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        let mut conditions = Vec::new();
        let mut bind_values: Vec<String> = Vec::new();

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
                "(filename LIKE ? OR sys_name LIKE ? OR ver_hw LIKE ?)".to_string(),
            );
            let pattern = format!("%{}%", search);
            bind_values.push(pattern.clone());
            bind_values.push(pattern.clone());
            bind_values.push(pattern);
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

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
            "SELECT * FROM logs {} ORDER BY created_at DESC LIMIT ? OFFSET ?",
            where_clause
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

    async fn delete(&self, id: Uuid) -> Result<bool, DbError> {
        let id_str = id.to_string();
        let result = sqlx::query("DELETE FROM logs WHERE id = ?")
            .bind(&id_str)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
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
}
