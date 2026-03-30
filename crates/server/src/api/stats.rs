use axum::{extract::{Query, State}, Json};
use serde::Serialize;
use std::sync::Arc;

use super::ApiError;
use crate::db::{StatRow, StatsParams};

const ALLOWED_GROUP_BY: &[&str] = &[
    "vehicle_type",
    "ver_hw",
    "ver_sw_release_str",
    "source",
    "vibration_status",
    "sys_name",
    "mission_type",
];

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub group_by: String,
    pub period: String,
    pub data: Vec<StatRow>,
}

pub async fn get_stats(
    State(state): State<Arc<crate::AppState>>,
    Query(params): Query<StatsParams>,
) -> Result<Json<StatsResponse>, ApiError> {
    // Validate group_by against allowlist
    if !ALLOWED_GROUP_BY.contains(&params.group_by.as_str()) {
        return Err(ApiError::BadRequest(format!(
            "invalid group_by: '{}'. Allowed: {:?}",
            params.group_by, ALLOWED_GROUP_BY
        )));
    }

    let period_str = params
        .period
        .clone()
        .unwrap_or_else(|| "30d".to_string());

    let data = state.db.stats(&params).await?;

    Ok(Json(StatsResponse {
        group_by: params.group_by,
        period: period_str,
        data,
    }))
}
