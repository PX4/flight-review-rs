use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use super::ApiError;

/// GET /api/logs -- list with filters
pub async fn list_logs(
    State(state): State<Arc<crate::AppState>>,
    Query(filters): Query<crate::db::ListFilters>,
) -> Result<Json<crate::db::ListResponse>, ApiError> {
    let result = state.db.list(&filters).await?;
    Ok(Json(result))
}

/// GET /api/logs/:id -- single log metadata
pub async fn get_log(
    State(state): State<Arc<crate::AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::db::LogRecord>, ApiError> {
    match state.db.get(id).await? {
        Some(record) => Ok(Json(record)),
        None => Err(ApiError::NotFound),
    }
}

#[derive(Deserialize)]
pub struct DeleteParams {
    pub token: String,
}

/// DELETE /api/logs/:id?token=<delete_token>
pub async fn delete_log(
    State(state): State<Arc<crate::AppState>>,
    Path(id): Path<Uuid>,
    Query(params): Query<DeleteParams>,
) -> Result<StatusCode, ApiError> {
    // Look up the log
    let record = state.db.get(id).await?.ok_or(ApiError::NotFound)?;

    // Verify token
    if record.delete_token != params.token {
        return Err(ApiError::Forbidden);
    }

    // Proceed with delete
    state.storage.delete_log_files(id).await?;
    state.db.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/logs/:id/data/:filename -- serve Parquet/metadata files
/// Supports HTTP Range requests for DuckDB-WASM compatibility.
pub async fn get_log_file(
    State(state): State<Arc<crate::AppState>>,
    Path((id, filename)): Path<(Uuid, String)>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let content_type = if filename.ends_with(".parquet") {
        "application/octet-stream"
    } else if filename.ends_with(".json") {
        "application/json"
    } else {
        "application/octet-stream"
    };

    // Check for Range header
    if let Some(range_value) = headers.get(header::RANGE) {
        let range_str = range_value
            .to_str()
            .map_err(|_| ApiError::BadRequest("invalid Range header".to_string()))?;

        let (start, end) = parse_byte_range(range_str)?;

        let data = state
            .storage
            .get_range(id, &filename, start..end)
            .await?;

        let content_range = format!("bytes {}-{}/{}", start, end.saturating_sub(1), "*");

        Ok((
            StatusCode::PARTIAL_CONTENT,
            [
                (header::CONTENT_TYPE, content_type.to_string()),
                (header::ACCEPT_RANGES, "bytes".to_string()),
                (header::CONTENT_RANGE, content_range),
                (header::CONTENT_LENGTH, data.len().to_string()),
            ],
            data,
        )
            .into_response())
    } else {
        let data = state.storage.get_file(id, &filename).await?;

        Ok((
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, content_type.to_string()),
                (header::ACCEPT_RANGES, "bytes".to_string()),
                (header::CONTENT_LENGTH, data.len().to_string()),
            ],
            data,
        )
            .into_response())
    }
}

/// Parse a simple `bytes=START-END` range header.
/// Returns (start, end_exclusive) for use with object_store get_range.
fn parse_byte_range(range_str: &str) -> Result<(u64, u64), ApiError> {
    let bytes_prefix = "bytes=";
    if !range_str.starts_with(bytes_prefix) {
        return Err(ApiError::BadRequest(format!(
            "unsupported range format: {range_str}"
        )));
    }

    let range_spec = &range_str[bytes_prefix.len()..];
    let parts: Vec<&str> = range_spec.splitn(2, '-').collect();
    if parts.len() != 2 {
        return Err(ApiError::BadRequest(format!(
            "invalid range: {range_str}"
        )));
    }

    let start: u64 = parts[0]
        .parse()
        .map_err(|_| ApiError::BadRequest(format!("invalid range start: {range_str}")))?;

    // END in HTTP Range is inclusive, but object_store uses exclusive end
    let end: u64 = parts[1]
        .parse::<u64>()
        .map_err(|_| ApiError::BadRequest(format!("invalid range end: {range_str}")))?
        + 1; // Convert inclusive end to exclusive

    Ok((start, end))
}
