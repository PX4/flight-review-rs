use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use bytes::Bytes;
use serde::Deserialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::extract::extract_search_fields;

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
/// If the file does not exist in v2 storage but a v1 ULG prefix is configured,
/// attempts lazy conversion of the original .ulg file on first access.
pub async fn get_log_file(
    State(state): State<Arc<crate::AppState>>,
    Path((id, filename)): Path<(Uuid, String)>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    let content_type = if filename.ends_with(".json") {
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

        // Try v2 storage first; lazy-convert on miss
        let data = match state.storage.get_range(id, &filename, start..end).await {
            Ok(d) => d,
            Err(_) if state.v1_ulg_prefix.is_some() => {
                let converted = lazy_convert(&state, id).await?;
                if !converted {
                    return Err(ApiError::NotFound);
                }
                state
                    .storage
                    .get_range(id, &filename, start..end)
                    .await
                    .map_err(|_| ApiError::NotFound)?
            }
            Err(_) => return Err(ApiError::NotFound),
        };

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
        // Try v2 storage first; lazy-convert on miss
        let data = match state.storage.get_file(id, &filename).await {
            Ok(d) => d,
            Err(_) if state.v1_ulg_prefix.is_some() => {
                let converted = lazy_convert(&state, id).await?;
                if !converted {
                    return Err(ApiError::NotFound);
                }
                state
                    .storage
                    .get_file(id, &filename)
                    .await
                    .map_err(|_| ApiError::NotFound)?
            }
            Err(_) => return Err(ApiError::NotFound),
        };

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

/// Lazily convert a v1 .ulg file to v2 Parquet on first access.
///
/// Returns `true` if conversion succeeded (or was already done), `false` if the
/// source .ulg file could not be found.
async fn lazy_convert(state: &crate::AppState, id: Uuid) -> Result<bool, ApiError> {
    let v1_prefix = state.v1_ulg_prefix.as_deref().unwrap();

    // Idempotency check: if metadata.json already exists, another request won
    // the race and conversion is done.
    if state.storage.get_file(id, "metadata.json").await.is_ok() {
        return Ok(true);
    }

    // Try to fetch the v1 .ulg file
    let ulg_path = format!("{}/{}.ulg", v1_prefix, id);
    let ulg_data = match state.storage.get_raw(&ulg_path).await {
        Ok(data) => data,
        Err(_) => return Ok(false),
    };

    let file_size = ulg_data.len() as i64;

    // Write to temp file and convert
    let tmp_dir =
        tempfile::tempdir().map_err(|e| ApiError::Internal(format!("tempdir: {e}")))?;
    let input_path = tmp_dir.path().join("input.ulg");
    tokio::fs::write(&input_path, &ulg_data).await?;

    let output_dir = tmp_dir.path().join("output");
    let input_str = input_path.to_string_lossy().to_string();
    let output_dir_clone = output_dir.clone();

    let result = tokio::task::spawn_blocking(move || {
        flight_review::converter::convert_ulog(&input_str, &output_dir_clone)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("spawn_blocking join error: {e}")))?
    .map_err(|e| ApiError::Internal(format!("conversion error: {e}")))?;

    // Upload Parquet files
    for parquet_path in &result.parquet_files {
        let fname = parquet_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ApiError::Internal("invalid parquet filename".to_string()))?;
        let file_data = tokio::fs::read(parquet_path).await?;
        state
            .storage
            .put_file(id, fname, Bytes::from(file_data))
            .await?;
    }

    // Upload metadata.json
    let metadata_json = serde_json::to_vec_pretty(&result.metadata)
        .map_err(|e| ApiError::Internal(format!("metadata serialization: {e}")))?;
    state
        .storage
        .put_file(id, "metadata.json", Bytes::from(metadata_json))
        .await?;

    // Copy the raw .ulg into the UUID directory so everything lives together
    let ulg_filename = format!("{}.ulg", id);
    state
        .storage
        .put_file(id, &ulg_filename, ulg_data)
        .await?;

    // Update the DB record with fields extracted from the conversion
    if let Some(mut record) = state.db.get(id).await? {
        record.sys_name = result.metadata.sys_name.clone().or(record.sys_name);
        record.ver_hw = result.metadata.ver_hw.clone().or(record.ver_hw);
        record.ver_sw_release_str = result
            .metadata
            .ver_sw_release_str
            .clone()
            .or(record.ver_sw_release_str);
        record.flight_duration_s = result.metadata.flight_duration_s.or(record.flight_duration_s);
        record.topic_count = result.metadata.topics.len() as i32;
        record.lat = result
            .metadata
            .gps_first_fix
            .as_ref()
            .map(|g| g.lat_deg)
            .or(record.lat);
        record.lon = result
            .metadata
            .gps_first_fix
            .as_ref()
            .map(|g| g.lon_deg)
            .or(record.lon);
        record.file_size = file_size;

        let search = extract_search_fields(&result.metadata);
        record.sys_uuid = search.sys_uuid.or(record.sys_uuid);
        record.ver_sw = search.ver_sw.or(record.ver_sw);
        record.vehicle_type = search.vehicle_type.or(record.vehicle_type);
        record.localization_sources = search.localization_sources.or(record.localization_sources);
        record.vibration_status = search.vibration_status.or(record.vibration_status);
        record.battery_min_voltage = search.battery_min_voltage.or(record.battery_min_voltage);
        record.gps_max_eph = search.gps_max_eph.or(record.gps_max_eph);
        record.max_speed_m_s = search.max_speed_m_s.or(record.max_speed_m_s);
        record.total_distance_m = search.total_distance_m.or(record.total_distance_m);
        record.error_count = search.error_count.or(record.error_count);
        record.warning_count = search.warning_count.or(record.warning_count);

        state.db.update(id, &record).await?;
    }

    // Insert field stats from analysis
    if let Some(ref analysis) = result.metadata.analysis {
        let field_stats: Vec<crate::db::FieldStatRecord> = analysis
            .field_stats
            .iter()
            .map(|fs| crate::db::FieldStatRecord {
                topic: fs.topic.clone(),
                field: fs.field.clone(),
                min_val: fs.min,
                max_val: fs.max,
                mean_val: fs.mean,
                count: fs.count as i64,
            })
            .collect();
        state.db.insert_field_stats(id, &field_stats).await?;
    }

    tracing::info!(
        log_id = %id,
        topics = result.metadata.topics.len(),
        parquet_files = result.parquet_files.len(),
        "lazy conversion complete"
    );

    Ok(true)
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
