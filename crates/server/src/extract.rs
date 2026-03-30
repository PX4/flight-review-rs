//! Extract search-relevant fields from [`FlightMetadata`] for DB indexing.
//!
//! This module bridges the converter crate's metadata types to flat columns
//! that can be stored in the database for efficient indexed search.

use flight_review::metadata::{FlightMetadata, ParamValue};

/// Search-relevant fields extracted from FlightMetadata.
/// These get promoted to DB columns for indexed search.
pub struct SearchFields {
    pub sys_uuid: Option<String>,
    pub ver_sw: Option<String>,
    pub vehicle_type: Option<String>,
    pub localization_sources: Option<String>,
    pub vibration_status: Option<String>,
    pub battery_min_voltage: Option<f64>,
    pub gps_max_eph: Option<f64>,
    pub max_speed_m_s: Option<f64>,
    pub total_distance_m: Option<f64>,
    pub error_count: Option<i32>,
    pub warning_count: Option<i32>,
}

/// Derive all 11 search column values from a [`FlightMetadata`] struct.
pub fn extract_search_fields(metadata: &FlightMetadata) -> SearchFields {
    SearchFields {
        sys_uuid: metadata.sys_uuid.clone(),
        ver_sw: metadata.ver_sw.clone(),
        vehicle_type: detect_vehicle_type(metadata),
        localization_sources: detect_localization(metadata),
        vibration_status: extract_vibration_status(metadata),
        battery_min_voltage: extract_battery_min_voltage(metadata),
        gps_max_eph: extract_gps_max_eph(metadata),
        max_speed_m_s: extract_max_speed(metadata),
        total_distance_m: extract_total_distance(metadata),
        error_count: Some(count_errors(metadata)),
        warning_count: Some(count_warnings(metadata)),
    }
}

/// Detect vehicle type from the MAV_TYPE parameter.
///
/// MAV_TYPE values: 1=Fixed Wing, 2=Quadrotor, 3=Coaxial, 4=Helicopter,
/// 10=Ground Rover, 11=Surface Boat, 12=Submarine, 13=Hexarotor,
/// 14=Octorotor, 15=Tricopter, 19=VTOL Tailsitter Duo,
/// 20=VTOL Tailsitter Quad, 21=VTOL Tiltrotor, 22=VTOL Standard.
fn detect_vehicle_type(metadata: &FlightMetadata) -> Option<String> {
    let mav_type = match metadata.parameters.get("MAV_TYPE") {
        Some(ParamValue::Int32(v)) => *v,
        Some(ParamValue::Float(v)) => *v as i32,
        None => return None,
    };

    Some(
        match mav_type {
            1 => "Fixed Wing",
            2 | 3 | 4 | 13 | 14 | 15 => "Multirotor",
            10 => "Rover",
            11 => "Boat",
            12 => "Submarine",
            19 | 20 | 21 | 22 => "VTOL",
            _ => "Other",
        }
        .to_string(),
    )
}

/// Detect localization sources from EKF2_AID_MASK parameter and topic presence.
fn detect_localization(metadata: &FlightMetadata) -> Option<String> {
    let mut sources = Vec::new();

    // From EKF2_AID_MASK parameter bitmask
    if let Some(param) = metadata.parameters.get("EKF2_AID_MASK") {
        let mask = match param {
            ParamValue::Int32(v) => *v,
            ParamValue::Float(v) => *v as i32,
        };
        if mask & 1 != 0 {
            sources.push("GPS");
        }
        if mask & 2 != 0 {
            sources.push("OpticalFlow");
        }
        if mask & 8 != 0 {
            sources.push("Vision");
        }
    }

    // From topic presence (supplement what params don't tell us)
    if metadata.topics.contains_key("vehicle_visual_odometry") && !sources.contains(&"Vision") {
        sources.push("Vision");
    }
    if metadata.topics.contains_key("vehicle_gps_position") && !sources.contains(&"GPS") {
        sources.push("GPS");
    }
    if metadata.topics.contains_key("vehicle_mocap_attitude") {
        sources.push("Mocap");
    }
    if metadata.topics.contains_key("optical_flow") && !sources.contains(&"OpticalFlow") {
        sources.push("OpticalFlow");
    }
    if metadata.topics.contains_key("distance_sensor") {
        sources.push("RangeFinder");
    }

    if sources.is_empty() {
        None
    } else {
        Some(sources.join(","))
    }
}

/// Extract vibration status from analysis. Returns None if analysis is absent
/// or the status string is empty (no vibration data collected).
fn extract_vibration_status(metadata: &FlightMetadata) -> Option<String> {
    metadata
        .analysis
        .as_ref()
        .map(|a| a.vibration.status.clone())
        .filter(|s| !s.is_empty())
}

/// Extract minimum battery voltage from analysis.
fn extract_battery_min_voltage(metadata: &FlightMetadata) -> Option<f64> {
    metadata
        .analysis
        .as_ref()
        .and_then(|a| a.battery.min_voltage_v)
}

/// Extract maximum GPS horizontal position error from analysis.
fn extract_gps_max_eph(metadata: &FlightMetadata) -> Option<f64> {
    metadata
        .analysis
        .as_ref()
        .and_then(|a| a.gps_quality.max_eph_m.map(|v| v as f64))
}

/// Extract maximum 3D speed from analysis. Returns None if zero (no movement).
fn extract_max_speed(metadata: &FlightMetadata) -> Option<f64> {
    metadata
        .analysis
        .as_ref()
        .map(|a| a.stats.max_speed_m_s)
        .filter(|v| *v > 0.0)
}

/// Extract total distance from analysis. Returns None if zero (no movement).
fn extract_total_distance(metadata: &FlightMetadata) -> Option<f64> {
    metadata
        .analysis
        .as_ref()
        .map(|a| a.stats.total_distance_m)
        .filter(|v| *v > 0.0)
}

/// Count logged messages at ERROR level or above (EMERGENCY, ALERT, CRITICAL, ERROR).
fn count_errors(metadata: &FlightMetadata) -> i32 {
    metadata
        .logged_messages
        .iter()
        .filter(|m| {
            matches!(
                m.level.as_str(),
                "EMERGENCY" | "ALERT" | "CRITICAL" | "ERROR"
            )
        })
        .count() as i32
}

/// Count logged messages at WARNING level.
fn count_warnings(metadata: &FlightMetadata) -> i32 {
    metadata
        .logged_messages
        .iter()
        .filter(|m| m.level == "WARNING")
        .count() as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use flight_review::metadata::extract_metadata;

    fn fixture(name: &str) -> String {
        let manifest = env!("CARGO_MANIFEST_DIR");

        // First: check local fixtures in the converter crate
        let local = std::path::Path::new(manifest)
            .parent().unwrap()  // crates/
            .parent().unwrap()  // workspace root
            .join("crates/converter/tests/fixtures")
            .join(name);
        if local.exists() {
            return local.to_string_lossy().to_string();
        }

        // Fallback: px4-ulog-rs repo (local dev)
        let external = std::path::Path::new(manifest)
            .parent().unwrap()  // crates/
            .parent().unwrap()  // workspace root
            .parent().unwrap()  // ulog/
            .join("px4-ulog-rs/tests/fixtures")
            .join(name);
        external.to_string_lossy().to_string()
    }

    #[test]
    fn test_extract_fixed_wing() {
        let path = fixture("fixed_wing_gps.ulg");
        if !std::path::Path::new(&path).exists() {
            eprintln!("Skipping: fixed_wing_gps.ulg not available");
            return;
        }
        let mut meta = extract_metadata(&path).unwrap();
        let analysis = flight_review::analysis::analyze(&path, &meta).unwrap();
        meta.analysis = Some(analysis);

        let fields = extract_search_fields(&meta);

        // Should detect vehicle type from MAV_TYPE param
        assert!(fields.vehicle_type.is_some());
        // Should have GPS in localization
        assert!(
            fields
                .localization_sources
                .as_ref()
                .unwrap()
                .contains("GPS")
        );
        // Should have vibration status
        assert!(fields.vibration_status.is_some());
        // Should have error/warning counts
        assert!(fields.error_count.is_some());
        assert!(fields.warning_count.is_some());
    }

    #[test]
    fn test_extract_sample() {
        let path = fixture("sample.ulg");
        let mut meta = extract_metadata(&path).unwrap();
        let analysis = flight_review::analysis::analyze(&path, &meta).unwrap();
        meta.analysis = Some(analysis);

        let fields = extract_search_fields(&meta);
        assert!(fields.error_count.is_some());
    }
}
