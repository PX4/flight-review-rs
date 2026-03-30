use super::{DbError, ListFilters, ListResponse, LogRecord, LogStore};
use uuid::Uuid;

#[cfg(feature = "postgres")]
use sqlx::postgres::PgPoolOptions;
#[cfg(feature = "postgres")]
use sqlx::{PgPool, Row};

#[cfg(feature = "postgres")]
const CREATE_TABLE: &str = r#"
CREATE TABLE IF NOT EXISTS logs (
    id UUID PRIMARY KEY NOT NULL,
    filename TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    file_size BIGINT NOT NULL DEFAULT 0,
    sys_name TEXT,
    ver_hw TEXT,
    ver_sw_release_str TEXT,
    flight_duration_s DOUBLE PRECISION,
    topic_count INTEGER NOT NULL DEFAULT 0,
    lat DOUBLE PRECISION,
    lon DOUBLE PRECISION,
    is_public BOOLEAN NOT NULL DEFAULT false,
    delete_token TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_logs_created_at ON logs(created_at);
CREATE INDEX IF NOT EXISTS idx_logs_sys_name ON logs(sys_name);
CREATE INDEX IF NOT EXISTS idx_logs_ver_hw ON logs(ver_hw);
"#;

#[cfg(feature = "postgres")]
pub struct PostgresStore {
    pool: PgPool,
}

#[cfg(not(feature = "postgres"))]
pub struct PostgresStore {
    _phantom: std::marker::PhantomData<()>,
}

#[cfg(feature = "postgres")]
impl PostgresStore {
    pub async fn new(url: &str) -> Result<Self, DbError> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await?;

        sqlx::query(CREATE_TABLE).execute(&pool).await?;

        Ok(Self { pool })
    }
}

#[cfg(not(feature = "postgres"))]
impl PostgresStore {
    pub async fn new(_url: &str) -> Result<Self, DbError> {
        Err(DbError::Sqlx(sqlx::Error::Configuration(
            "postgres feature is not enabled".into(),
        )))
    }
}

#[cfg(feature = "postgres")]
fn row_to_record(row: &sqlx::postgres::PgRow) -> Result<LogRecord, sqlx::Error> {
    Ok(LogRecord {
        id: row.try_get("id")?,
        filename: row.try_get("filename")?,
        created_at: row.try_get("created_at")?,
        file_size: row.try_get("file_size")?,
        sys_name: row.try_get("sys_name")?,
        ver_hw: row.try_get("ver_hw")?,
        ver_sw_release_str: row.try_get("ver_sw_release_str")?,
        flight_duration_s: row.try_get("flight_duration_s")?,
        topic_count: row.try_get("topic_count")?,
        lat: row.try_get("lat")?,
        lon: row.try_get("lon")?,
        is_public: row.try_get("is_public")?,
        delete_token: row.try_get("delete_token")?,
    })
}

#[cfg(feature = "postgres")]
#[async_trait::async_trait]
impl LogStore for PostgresStore {
    async fn insert(&self, record: &LogRecord) -> Result<(), DbError> {
        sqlx::query(
            "INSERT INTO logs (id, filename, created_at, file_size, sys_name, ver_hw, \
             ver_sw_release_str, flight_duration_s, topic_count, lat, lon, is_public, delete_token) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(record.id)
        .bind(&record.filename)
        .bind(record.created_at)
        .bind(record.file_size)
        .bind(&record.sys_name)
        .bind(&record.ver_hw)
        .bind(&record.ver_sw_release_str)
        .bind(record.flight_duration_s)
        .bind(record.topic_count)
        .bind(record.lat)
        .bind(record.lon)
        .bind(record.is_public)
        .bind(&record.delete_token)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<LogRecord>, DbError> {
        let row = sqlx::query("SELECT * FROM logs WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(r) => Ok(Some(row_to_record(&r)?)),
            None => Ok(None),
        }
    }

    async fn list(&self, filters: &ListFilters) -> Result<ListResponse, DbError> {
        let mut conditions = Vec::new();
        let mut param_idx: usize = 1;

        // We'll build the query string with numbered parameters and collect bind values.
        // Since sqlx doesn't support heterogeneous bind lists easily with dynamic queries,
        // we collect string values and bind them in order.
        let mut bind_values: Vec<String> = Vec::new();

        // By default only return public logs
        if !filters.include_private.unwrap_or(false) {
            conditions.push("is_public = true".to_string());
        }

        if let Some(ref sys_name) = filters.sys_name {
            conditions.push(format!("sys_name = ${}", param_idx));
            bind_values.push(sys_name.clone());
            param_idx += 1;
        }
        if let Some(ref ver_hw) = filters.ver_hw {
            conditions.push(format!("ver_hw = ${}", param_idx));
            bind_values.push(ver_hw.clone());
            param_idx += 1;
        }
        if let Some(ref search) = filters.search {
            conditions.push(format!(
                "(filename ILIKE ${p} OR sys_name ILIKE ${p1} OR ver_hw ILIKE ${p2})",
                p = param_idx,
                p1 = param_idx + 1,
                p2 = param_idx + 2,
            ));
            let pattern = format!("%{}%", search);
            bind_values.push(pattern.clone());
            bind_values.push(pattern.clone());
            bind_values.push(pattern);
            param_idx += 3;
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let limit = filters.limit.unwrap_or(50);
        let offset = filters.offset.unwrap_or(0);

        // Count query
        let count_sql = format!("SELECT COUNT(*) as cnt FROM logs {}", where_clause);
        let mut count_query = sqlx::query(&count_sql);
        for v in &bind_values {
            count_query = count_query.bind(v);
        }
        let count_row = count_query.fetch_one(&self.pool).await?;
        let total: i64 = count_row.try_get("cnt")?;

        // Data query
        let data_sql = format!(
            "SELECT * FROM logs {} ORDER BY created_at DESC LIMIT ${} OFFSET ${}",
            where_clause, param_idx, param_idx + 1
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
        let result = sqlx::query("DELETE FROM logs WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    async fn update(&self, id: Uuid, record: &LogRecord) -> Result<(), DbError> {
        sqlx::query(
            "UPDATE logs SET filename = $1, created_at = $2, file_size = $3, sys_name = $4, ver_hw = $5, \
             ver_sw_release_str = $6, flight_duration_s = $7, topic_count = $8, lat = $9, lon = $10, \
             is_public = $11, delete_token = $12 WHERE id = $13",
        )
        .bind(&record.filename)
        .bind(record.created_at)
        .bind(record.file_size)
        .bind(&record.sys_name)
        .bind(&record.ver_hw)
        .bind(&record.ver_sw_release_str)
        .bind(record.flight_duration_s)
        .bind(record.topic_count)
        .bind(record.lat)
        .bind(record.lon)
        .bind(record.is_public)
        .bind(&record.delete_token)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

#[cfg(not(feature = "postgres"))]
#[async_trait::async_trait]
impl LogStore for PostgresStore {
    async fn insert(&self, _record: &LogRecord) -> Result<(), DbError> {
        Err(DbError::Sqlx(sqlx::Error::Configuration(
            "postgres feature is not enabled".into(),
        )))
    }

    async fn get(&self, _id: Uuid) -> Result<Option<LogRecord>, DbError> {
        Err(DbError::Sqlx(sqlx::Error::Configuration(
            "postgres feature is not enabled".into(),
        )))
    }

    async fn list(&self, _filters: &ListFilters) -> Result<ListResponse, DbError> {
        Err(DbError::Sqlx(sqlx::Error::Configuration(
            "postgres feature is not enabled".into(),
        )))
    }

    async fn delete(&self, _id: Uuid) -> Result<bool, DbError> {
        Err(DbError::Sqlx(sqlx::Error::Configuration(
            "postgres feature is not enabled".into(),
        )))
    }

    async fn update(&self, _id: Uuid, _record: &LogRecord) -> Result<(), DbError> {
        Err(DbError::Sqlx(sqlx::Error::Configuration(
            "postgres feature is not enabled".into(),
        )))
    }
}
