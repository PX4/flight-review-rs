use axum::{extract::Multipart, extract::State, Json};
use bytes::Bytes;
use std::sync::Arc;
use uuid::Uuid;

use super::ApiError;

pub async fn upload(
    State(state): State<Arc<crate::AppState>>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, ApiError> {
    // 1. Extract the file and optional fields from multipart
    let mut file_bytes: Option<(String, Bytes)> = None;
    let mut is_public = false;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("multipart error: {e}")))?
    {
        if field.name() == Some("file") {
            let filename = field
                .file_name()
                .unwrap_or("upload.ulg")
                .to_string();
            let data = field
                .bytes()
                .await
                .map_err(|e| ApiError::BadRequest(format!("failed to read file field: {e}")))?;
            file_bytes = Some((filename, data));
        } else if field.name() == Some("is_public") {
            let val = field.text().await.unwrap_or_default();
            is_public = val == "true" || val == "1";
        }
    }

    let (original_filename, data) = file_bytes.ok_or_else(|| {
        ApiError::BadRequest("missing 'file' field in multipart form".to_string())
    })?;

    let file_size = data.len() as i64;

    // 2. Save to a temp file and run conversion
    let tmp_dir = tempfile::tempdir().map_err(|e| ApiError::Internal(format!("tempdir: {e}")))?;
    let input_path = tmp_dir.path().join("input.ulg");
    tokio::fs::write(&input_path, &data).await?;

    let output_dir = tmp_dir.path().join("output");
    let input_str = input_path.to_string_lossy().to_string();
    let output_dir_clone = output_dir.clone();

    // 3. Generate a UUID for this log
    let log_id = Uuid::new_v4();

    // 4. Run convert_ulog in a blocking task (CPU-bound)
    let result = tokio::task::spawn_blocking(move || {
        flight_review::converter::convert_ulog(&input_str, &output_dir_clone)
    })
    .await
    .map_err(|e| ApiError::Internal(format!("spawn_blocking join error: {e}")))?
    .map_err(|e| ApiError::Internal(format!("conversion error: {e}")))?;

    // 5. Store each Parquet file in object storage under log_id/
    for parquet_path in &result.parquet_files {
        let filename = parquet_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| ApiError::Internal("invalid parquet filename".to_string()))?;
        let file_data = tokio::fs::read(parquet_path).await?;
        state
            .storage
            .put_file(log_id, filename, Bytes::from(file_data))
            .await?;
    }

    // Store metadata.json alongside the Parquet files
    let metadata_json = serde_json::to_vec_pretty(&result.metadata)
        .map_err(|e| ApiError::Internal(format!("metadata serialization: {e}")))?;
    state
        .storage
        .put_file(log_id, "metadata.json", Bytes::from(metadata_json))
        .await?;

    // 6. Create a LogRecord from the metadata and insert into DB
    let delete_token = Uuid::new_v4().simple().to_string();
    let record = crate::db::LogRecord {
        id: log_id,
        filename: original_filename,
        created_at: chrono::Utc::now(),
        file_size,
        sys_name: result.metadata.sys_name.clone(),
        ver_hw: result.metadata.ver_hw.clone(),
        ver_sw_release_str: result.metadata.ver_sw_release_str.clone(),
        flight_duration_s: result.metadata.flight_duration_s,
        topic_count: result.metadata.topics.len() as i32,
        lat: result.metadata.gps_first_fix.as_ref().map(|g| g.lat_deg),
        lon: result.metadata.gps_first_fix.as_ref().map(|g| g.lon_deg),
        is_public,
        delete_token: delete_token.clone(),
    };

    state.db.insert(&record).await?;

    // 7. Temp files cleaned up when tmp_dir is dropped

    tracing::info!(
        log_id = %log_id,
        filename = %record.filename,
        topics = result.metadata.topics.len(),
        parquet_files = result.parquet_files.len(),
        "upload complete"
    );

    // 8. Return JSON with log id and metadata summary
    Ok(Json(serde_json::json!({
        "id": log_id,
        "filename": record.filename,
        "sys_name": record.sys_name,
        "ver_hw": record.ver_hw,
        "flight_duration_s": record.flight_duration_s,
        "topic_count": record.topic_count,
        "is_public": is_public,
        "delete_token": delete_token,
        "parquet_files": result.parquet_files.iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
            .collect::<Vec<_>>(),
    })))
}
