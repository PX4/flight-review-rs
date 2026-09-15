//! Extract flight analysis data from ULog files.
//!
//! Performs a streaming pass through the ULog file, extracting typed field
//! values from specific topics to produce flight statistics, mode timelines,
//! battery summaries, GPS quality metrics, vibration data, and GPS tracks.

use crate::metadata::{FlightMetadata, ParamValue};
use px4_ulog::stream_parser::file_reader::{
    read_file_with_simple_callback, Message, SimpleCallbackResult,
};
use px4_ulog::stream_parser::model::{DataMessage, FlattenedFieldType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FlightAnalysis {
    pub flight_modes: Vec<FlightModeSegment>,
    pub vtol_states: Vec<VtolStateSegment>,
    pub stats: FlightStats,
    pub battery: BatterySummary,
    pub gps_quality: GpsQuality,
    pub vibration: VibrationSummary,
    pub non_default_params: Vec<ParamDiff>,
    pub gps_track: Vec<TrackPoint>,
    /// Per-topic-field statistics (min, max, mean), using ULog instance zero only.
    pub field_stats: Vec<FieldStat>,
    /// Diagnostic anomalies detected during analysis.
    #[serde(default)]
    pub diagnostics: Vec<crate::diagnostics::Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldStat {
    pub topic: String,
    pub field: String,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlightModeSegment {
    pub mode: String,
    pub mode_id: u8,
    pub start_us: u64,
    pub end_us: u64,
    pub duration_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VtolStateSegment {
    pub state: String, // "MC", "FW", "Transition"
    pub start_us: u64,
    pub end_us: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FlightStats {
    /// Recorded path length from valid components of instance-zero local position.
    /// Invalid intervals and estimator reset boundaries are not bridged.
    pub total_distance_m: f64,
    /// Largest valid vertical span within a continuous estimator coordinate frame.
    pub max_altitude_diff_m: f64,
    pub max_speed_m_s: f64,
    pub max_horizontal_speed_m_s: f64,
    pub max_speed_up_m_s: f64,
    pub max_speed_down_m_s: f64,
    pub avg_speed_m_s: f64,
    pub max_tilt_deg: f64,
    pub max_rotation_speed_deg_s: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BatterySummary {
    pub avg_current_a: Option<f64>,
    pub max_current_a: Option<f64>,
    pub discharged_mah: Option<f64>,
    pub min_voltage_v: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GpsQuality {
    pub min_satellites: Option<u16>,
    pub max_satellites: Option<u16>,
    pub max_hdop: Option<f32>,
    pub max_eph_m: Option<f32>,
    pub max_epv_m: Option<f32>,
    pub fix_types_seen: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VibrationSummary {
    pub accel_vibe_mean: Option<f64>,
    pub accel_vibe_max: Option<f64>,
    /// Worst of the mean and peak classifications.
    /// Mean thresholds: good < 4.905, warning < 9.81, critical >= 9.81.
    /// Max thresholds:  good < 9.81,  warning < 19.62, critical >= 19.62.
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDiff {
    pub name: String,
    pub value: f64,
    pub default: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackPoint {
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub alt_m: f64,
    pub timestamp_us: u64,
    pub mode_id: u8,
}

/// Maximum number of non-timestamp numeric fields to track per topic.
const MAX_FIELDS_PER_TOPIC: usize = 10;

/// Running statistics accumulator for a single field.
struct RunningStats {
    topic: String,
    field: String,
    min: f64,
    max: f64,
    sum: f64,
    count: u64,
    /// Offset into the data message for this field.
    offset: u16,
    /// The field type, used to parse the raw bytes.
    field_type: FlattenedFieldType,
}

impl RunningStats {
    fn new(topic: String, field: String, offset: u16, field_type: FlattenedFieldType) -> Self {
        Self {
            topic,
            field,
            min: f64::MAX,
            max: f64::MIN,
            sum: 0.0,
            count: 0,
            offset,
            field_type,
        }
    }

    /// Parse the value from raw message bytes and update running stats.
    fn update(&mut self, data: &[u8]) {
        let off = self.offset as usize;
        if off >= data.len() {
            return;
        }
        let val = match self.field_type {
            FlattenedFieldType::Float => {
                if off + 4 > data.len() {
                    return;
                }
                f32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as f64
            }
            FlattenedFieldType::Double => {
                if off + 8 > data.len() {
                    return;
                }
                f64::from_le_bytes(data[off..off + 8].try_into().unwrap())
            }
            FlattenedFieldType::Int32 => {
                if off + 4 > data.len() {
                    return;
                }
                i32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as f64
            }
            FlattenedFieldType::UInt32 => {
                if off + 4 > data.len() {
                    return;
                }
                u32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as f64
            }
            FlattenedFieldType::Int16 => {
                if off + 2 > data.len() {
                    return;
                }
                i16::from_le_bytes(data[off..off + 2].try_into().unwrap()) as f64
            }
            FlattenedFieldType::UInt16 => {
                if off + 2 > data.len() {
                    return;
                }
                u16::from_le_bytes(data[off..off + 2].try_into().unwrap()) as f64
            }
            FlattenedFieldType::Int8 => data[off] as i8 as f64,
            FlattenedFieldType::UInt8 => data[off] as f64,
            FlattenedFieldType::Int64 => {
                if off + 8 > data.len() {
                    return;
                }
                i64::from_le_bytes(data[off..off + 8].try_into().unwrap()) as f64
            }
            FlattenedFieldType::UInt64 => {
                if off + 8 > data.len() {
                    return;
                }
                u64::from_le_bytes(data[off..off + 8].try_into().unwrap()) as f64
            }
            FlattenedFieldType::Bool | FlattenedFieldType::Char => return,
        };

        if !val.is_finite() {
            return;
        }

        if val < self.min {
            self.min = val;
        }
        if val > self.max {
            self.max = val;
        }
        self.sum += val;
        self.count += 1;
    }

    fn into_field_stat(self) -> Option<FieldStat> {
        if self.count == 0 {
            return None;
        }
        Some(FieldStat {
            topic: self.topic,
            field: self.field,
            min: self.min,
            max: self.max,
            mean: self.sum / self.count as f64,
            count: self.count,
        })
    }
}

/// Returns true if a field type is numeric (should be tracked for stats).
fn is_numeric_field_type(ft: &FlattenedFieldType) -> bool {
    !matches!(ft, FlattenedFieldType::Bool | FlattenedFieldType::Char)
}

pub(crate) fn nav_state_name(id: u8) -> &'static str {
    match id {
        0 => "Manual",
        1 => "Altitude",
        2 => "Position",
        3 => "Mission",
        4 => "Loiter",
        5 => "RTL",
        10 => "Acro",
        12 => "Descend",
        13 => "Terminate",
        14 => "Offboard",
        15 => "Stabilized",
        17 => "Takeoff",
        18 => "Land",
        19 => "Follow",
        20 => "Precision Land",
        21 => "Orbit",
        22 => "VTOL Takeoff",
        _ => "Unknown",
    }
}

fn vtol_state_name(vehicle_type: u8, in_transition: bool) -> &'static str {
    if in_transition {
        "Transition"
    } else {
        match vehicle_type {
            1 => "MC",
            2 => "FW",
            _ => "Unknown",
        }
    }
}

fn bool_field(data: &DataMessage<'_>, name: &str) -> Option<bool> {
    crate::diagnostics::parse_bool(data, name)
}

fn component_valid(data: &DataMessage<'_>, name: &str) -> bool {
    // Schemas predating validity flags remain usable. A present but unsupported
    // flag type is not evidence that a component is valid.
    if data
        .flattened_format
        .field_iter()
        .any(|f| f.flattened_field_name == name)
    {
        bool_field(data, name).unwrap_or(false)
    } else {
        true
    }
}

#[derive(Default)]
struct PositionStats {
    prev_xy: Option<(f64, f64)>,
    prev_z: Option<f64>,
    xy_reset: Option<u8>,
    z_reset: Option<u8>,
    last_ts: Option<u64>,
    z_range: Option<(f64, f64)>,
    distance: f64,
    altitude_span: f64,
    speed_max: f64,
    horizontal_max: f64,
    up_max: f64,
    down_max: f64,
    speed_sum: f64,
    speed_count: u64,
}

impl PositionStats {
    fn update(&mut self, data: &DataMessage<'_>, ts: Option<u64>) {
        let Some(ts) = ts else { return };
        if self.last_ts.is_some_and(|last| ts <= last) {
            return;
        }
        self.last_ts = Some(ts);
        let value = |name| {
            data.flattened_format
                .get_field_parser::<f32>(name)
                .ok()
                .map(|p| f64::from(p.parse(data.data)))
                .filter(|v| v.is_finite())
        };
        let counter = |name| {
            data.flattened_format
                .get_field_parser::<u8>(name)
                .ok()
                .map(|p| p.parse(data.data))
        };
        let xy_reset = counter("xy_reset_counter");
        let z_reset = counter("z_reset_counter");
        if self.xy_reset != xy_reset {
            self.prev_xy = None;
        }
        if self.z_reset != z_reset {
            self.prev_z = None;
            self.z_range = None;
        }
        self.xy_reset = xy_reset;
        self.z_reset = z_reset;

        let xy = value("x")
            .zip(value("y"))
            .filter(|_| component_valid(data, "xy_valid"));
        let z = value("z").filter(|_| component_valid(data, "z_valid"));
        let horizontal_distance_sq = xy
            .zip(self.prev_xy)
            .map(|((x, y), (px, py))| (x - px).powi(2) + (y - py).powi(2));
        let vertical_distance_sq = z.zip(self.prev_z).map(|(z, pz)| (z - pz).powi(2));
        self.distance +=
            (horizontal_distance_sq.unwrap_or(0.0) + vertical_distance_sq.unwrap_or(0.0)).sqrt();
        self.prev_xy = xy;
        self.prev_z = z;
        if let Some(z) = z {
            let (min, max) = self.z_range.get_or_insert((z, z));
            *min = min.min(z);
            *max = max.max(z);
            self.altitude_span = self.altitude_span.max(*max - *min);
        } else {
            self.z_range = None;
        }

        let horizontal = value("vx")
            .zip(value("vy"))
            .filter(|_| component_valid(data, "v_xy_valid"))
            .map(|(vx, vy)| vx.hypot(vy));
        let vertical = value("vz").filter(|_| component_valid(data, "v_z_valid"));
        if let Some(speed) = horizontal {
            self.horizontal_max = self.horizontal_max.max(speed);
        }
        if let Some(vz) = vertical {
            self.up_max = self.up_max.max(-vz);
            self.down_max = self.down_max.max(vz);
        }
        if let (Some(horizontal), Some(vertical)) = (horizontal, vertical) {
            let speed = horizontal.hypot(vertical);
            self.speed_max = self.speed_max.max(speed);
            self.speed_sum += speed;
            self.speed_count += 1;
        }
    }
}

/// Analyze a ULog file, extracting flight statistics, mode timeline, battery
/// summary, GPS quality, vibration data, and GPS track.
///
/// This performs a second streaming pass through the file (the first pass is
/// done by `extract_metadata`). Missing topics are handled gracefully —
/// the corresponding fields in the result will be empty/default. General
/// summaries select ULog instance zero rather than pooling independent sensors
/// or estimator coordinate frames. Diagnostics still receive every instance.
pub fn analyze(path: &str, metadata: &FlightMetadata) -> Result<FlightAnalysis, std::io::Error> {
    let mut analysis = FlightAnalysis::default();

    // --- Non-default params (from metadata, no file pass needed) ---
    compute_non_default_params(metadata, &mut analysis);

    // --- Accumulators for streaming pass ---

    // Flight modes
    let mut current_nav_state: Option<u8> = None;
    let mut mode_start_us: u64 = 0;

    // VTOL state
    let mut current_vehicle_type: Option<u8> = None;
    let mut current_in_transition: Option<bool> = None;
    let mut vtol_start_us: u64 = 0;

    // Local position stats
    let mut position = PositionStats::default();
    let mut recorded_end_us: Option<u64> = None;
    let mut last_status_us: Option<u64> = None;

    // Attitude / tilt
    let mut max_tilt_rad: f32 = 0.0f32;

    // Angular velocity
    let mut max_rotation_speed_rad_s: f32 = 0.0f32;

    // Battery
    let mut current_sum: f64 = 0.0;
    let mut current_count: u64 = 0;
    let mut max_current: f32 = f32::MIN;
    let mut min_voltage: f32 = f32::MAX;
    let mut last_discharged: Option<f32> = None;
    let mut has_battery_data = false;

    // GPS quality
    let mut min_sats: u16 = u16::MAX;
    let mut max_sats: u16 = 0;
    let mut max_hdop: f32 = 0.0f32;
    let mut max_eph: f32 = 0.0f32;
    let mut max_epv: f32 = 0.0f32;
    let mut fix_types_seen: Vec<u8> = Vec::new();
    let mut has_gps_quality = false;

    // Vibration
    let mut vibe_sum: f64 = 0.0;
    let mut vibe_count: u64 = 0;
    let mut vibe_max: f32 = 0.0f32;

    // GPS track
    let mut last_track_ts: u64 = 0;

    // Build flight mode timeline reference for GPS track annotation
    // We'll finalize modes after the pass, so track raw mode changes
    let mut mode_changes: Vec<(u64, u8)> = Vec::new();

    // Diagnostic analyzers — piggyback on the same streaming pass
    let mut analyzers = crate::diagnostics::create_analyzers();
    let diagnostic_topics: HashSet<String> = analyzers
        .iter()
        .flat_map(|a| a.required_topics().iter().map(|s| s.to_string()))
        .collect();

    // Per-field stats: topic -> Vec<RunningStats>
    // On first message for each topic, discover numeric fields and create RunningStats entries.
    // Only instance zero contributes to these summaries.
    let mut field_stats_map: HashMap<String, Vec<RunningStats>> = HashMap::new();
    let mut topics_initialized: HashMap<String, bool> = HashMap::new();

    read_file_with_simple_callback(path, &mut |msg| {
        if let Message::Data(data) = msg {
            let topic = data.flattened_format.message_name.as_str();
            let ts = data
                .flattened_format
                .timestamp_field
                .as_ref()
                .map(|tf| tf.parse_timestamp(data.data));

            if let Some(ts) = ts {
                recorded_end_us = Some(recorded_end_us.unwrap_or(ts).max(ts));
            }
            // Dispatch before selecting the instance used by general summaries.
            if diagnostic_topics.contains(topic) {
                for analyzer in analyzers.iter_mut() {
                    if analyzer.required_topics().contains(&topic) {
                        analyzer.on_message(data);
                    }
                }
            }
            if data.multi_id.value() != 0 {
                return SimpleCallbackResult::KeepReading;
            }

            // --- Per-field stats tracking (all topics) ---
            if !topics_initialized.contains_key(topic) {
                topics_initialized.insert(topic.to_string(), true);
                let mut stats_vec = Vec::new();
                let mut numeric_count = 0;
                for field in data.flattened_format.field_iter() {
                    // Skip timestamp field
                    if field.flattened_field_name == "timestamp" {
                        continue;
                    }
                    if !is_numeric_field_type(&field.field_type) {
                        continue;
                    }
                    if numeric_count >= MAX_FIELDS_PER_TOPIC {
                        break;
                    }
                    stats_vec.push(RunningStats::new(
                        topic.to_string(),
                        field.flattened_field_name.clone(),
                        field.offset,
                        field.field_type.clone(),
                    ));
                    numeric_count += 1;
                }
                field_stats_map.insert(topic.to_string(), stats_vec);
            }

            if let Some(stats_vec) = field_stats_map.get_mut(topic) {
                for rs in stats_vec.iter_mut() {
                    rs.update(data.data);
                }
            }

            match topic {
                "vehicle_status" => {
                    if let Some(ts) = ts {
                        if last_status_us.is_some_and(|last| ts <= last) {
                            return SimpleCallbackResult::KeepReading;
                        }
                        last_status_us = Some(ts);
                        // nav_state
                        if let Ok(parser) =
                            data.flattened_format.get_field_parser::<u8>("nav_state")
                        {
                            let nav_state = parser.parse(data.data);
                            if current_nav_state != Some(nav_state) {
                                // Close previous segment
                                if let Some(prev) = current_nav_state {
                                    analysis.flight_modes.push(FlightModeSegment {
                                        mode: nav_state_name(prev).to_string(),
                                        mode_id: prev,
                                        start_us: mode_start_us,
                                        end_us: ts,
                                        duration_s: (ts - mode_start_us) as f64 / 1_000_000.0,
                                    });
                                }
                                current_nav_state = Some(nav_state);
                                mode_start_us = ts;
                                mode_changes.push((ts, nav_state));
                            }
                        }

                        // VTOL state
                        let vt = data
                            .flattened_format
                            .get_field_parser::<u8>("vehicle_type")
                            .ok()
                            .map(|p| p.parse(data.data));
                        // in_transition_mode can be bool or uint8
                        let in_trans = bool_field(data, "in_transition_mode");

                        if let (Some(vt_val), Some(it_val)) = (vt, in_trans) {
                            let changed = current_vehicle_type != Some(vt_val)
                                || current_in_transition != Some(it_val);
                            if changed {
                                // Close previous VTOL segment
                                if let Some(prev_vt) = current_vehicle_type {
                                    let prev_it = current_in_transition.unwrap_or(false);
                                    analysis.vtol_states.push(VtolStateSegment {
                                        state: vtol_state_name(prev_vt, prev_it).to_string(),
                                        start_us: vtol_start_us,
                                        end_us: ts,
                                    });
                                }
                                current_vehicle_type = Some(vt_val);
                                current_in_transition = Some(it_val);
                                vtol_start_us = ts;
                            }
                        }
                    }
                }

                "vehicle_local_position" => {
                    position.update(data, ts);
                }

                "vehicle_attitude" => {
                    // Quaternion fields are flattened as q[0], q[1], q[2], q[3]
                    if let (Ok(q0_p), Ok(q1_p), Ok(q2_p), Ok(q3_p)) = (
                        data.flattened_format.get_field_parser::<f32>("q[0]"),
                        data.flattened_format.get_field_parser::<f32>("q[1]"),
                        data.flattened_format.get_field_parser::<f32>("q[2]"),
                        data.flattened_format.get_field_parser::<f32>("q[3]"),
                    ) {
                        let q0 = q0_p.parse(data.data);
                        let q1 = q1_p.parse(data.data);
                        let q2 = q2_p.parse(data.data);
                        let q3 = q3_p.parse(data.data);

                        if q0.is_finite() && q1.is_finite() && q2.is_finite() && q3.is_finite() {
                            // Euler angles from quaternion
                            let roll =
                                (2.0 * (q0 * q1 + q2 * q3)).atan2(1.0 - 2.0 * (q1 * q1 + q2 * q2));
                            let sin_pitch = 2.0 * (q0 * q2 - q3 * q1);
                            let pitch = if sin_pitch.abs() >= 1.0 {
                                std::f32::consts::FRAC_PI_2.copysign(sin_pitch)
                            } else {
                                sin_pitch.asin()
                            };

                            let tilt = (pitch.cos() * roll.cos()).acos();
                            if tilt.is_finite() && tilt > max_tilt_rad {
                                max_tilt_rad = tilt;
                            }
                        }
                    }
                }

                "vehicle_angular_velocity" => {
                    // Angular velocity fields: xyz[0], xyz[1], xyz[2]
                    if let (Ok(x_p), Ok(y_p), Ok(z_p)) = (
                        data.flattened_format.get_field_parser::<f32>("xyz[0]"),
                        data.flattened_format.get_field_parser::<f32>("xyz[1]"),
                        data.flattened_format.get_field_parser::<f32>("xyz[2]"),
                    ) {
                        let wx = x_p.parse(data.data);
                        let wy = y_p.parse(data.data);
                        let wz = z_p.parse(data.data);
                        if wx.is_finite() && wy.is_finite() && wz.is_finite() {
                            let rot_speed = ((wx * wx + wy * wy + wz * wz) as f64).sqrt() as f32;
                            if rot_speed > max_rotation_speed_rad_s {
                                max_rotation_speed_rad_s = rot_speed;
                            }
                        }
                    }
                }

                "battery_status" => {
                    let current = data
                        .flattened_format
                        .get_field_parser::<f32>("current_a")
                        .ok()
                        .map(|p| p.parse(data.data));
                    let voltage = data
                        .flattened_format
                        .get_field_parser::<f32>("voltage_v")
                        .ok()
                        .map(|p| p.parse(data.data));
                    let discharged = data
                        .flattened_format
                        .get_field_parser::<f32>("discharged_mah")
                        .ok()
                        .map(|p| p.parse(data.data));

                    if let Some(c) = current {
                        if c.is_finite() && c >= 0.0 {
                            has_battery_data = true;
                            current_sum += c as f64;
                            current_count += 1;
                            if c > max_current {
                                max_current = c;
                            }
                        }
                    }
                    if let Some(v) = voltage {
                        if v.is_finite() && v > 0.0 {
                            has_battery_data = true;
                            if v < min_voltage {
                                min_voltage = v;
                            }
                        }
                    }
                    if let Some(d) = discharged {
                        if d.is_finite() && d >= 0.0 {
                            has_battery_data = true;
                            last_discharged = Some(d);
                        }
                    }
                }

                "vehicle_gps_position" => {
                    if let Some(ts) = ts {
                        // GPS quality
                        let sats = data
                            .flattened_format
                            .get_field_parser::<u8>("satellites_used")
                            .ok()
                            .map(|p| p.parse(data.data));
                        let fix_type = data
                            .flattened_format
                            .get_field_parser::<u8>("fix_type")
                            .ok()
                            .map(|p| p.parse(data.data));
                        let hdop = data
                            .flattened_format
                            .get_field_parser::<f32>("hdop")
                            .ok()
                            .map(|p| p.parse(data.data));
                        let eph = data
                            .flattened_format
                            .get_field_parser::<f32>("eph")
                            .ok()
                            .map(|p| p.parse(data.data));
                        let epv = data
                            .flattened_format
                            .get_field_parser::<f32>("epv")
                            .ok()
                            .map(|p| p.parse(data.data));

                        if let Some(s) = sats {
                            has_gps_quality = true;
                            let s16 = s as u16;
                            if s16 < min_sats {
                                min_sats = s16;
                            }
                            if s16 > max_sats {
                                max_sats = s16;
                            }
                        }
                        if let Some(ft) = fix_type {
                            has_gps_quality = true;
                            if !fix_types_seen.contains(&ft) {
                                fix_types_seen.push(ft);
                            }
                        }
                        if let Some(h) = hdop {
                            if h.is_finite() && h > max_hdop {
                                max_hdop = h;
                            }
                        }
                        if let Some(e) = eph {
                            if e.is_finite() && e > max_eph {
                                max_eph = e;
                            }
                        }
                        if let Some(e) = epv {
                            if e.is_finite() && e > max_epv {
                                max_epv = e;
                            }
                        }

                        // GPS track — downsample to ~1 Hz, only 3D fix or better
                        let ft = fix_type.unwrap_or(0);
                        if ft > 2 && ts.saturating_sub(last_track_ts) >= 1_000_000 {
                            // Try new field names (f64 degrees) first, fall back to legacy (i32 raw)
                            let coords: Option<(f64, f64, f64)> =
                                if let (Ok(lat_p), Ok(lon_p), Ok(alt_p)) = (
                                    data.flattened_format
                                        .get_field_parser::<f64>("latitude_deg"),
                                    data.flattened_format
                                        .get_field_parser::<f64>("longitude_deg"),
                                    data.flattened_format
                                        .get_field_parser::<f64>("altitude_msl_m"),
                                ) {
                                    let lat = lat_p.parse(data.data);
                                    let lon = lon_p.parse(data.data);
                                    let alt = alt_p.parse(data.data);
                                    Some((lat, lon, alt))
                                } else if let (Ok(lat_p), Ok(lon_p), Ok(alt_p)) = (
                                    data.flattened_format.get_field_parser::<i32>("lat"),
                                    data.flattened_format.get_field_parser::<i32>("lon"),
                                    data.flattened_format.get_field_parser::<i32>("alt"),
                                ) {
                                    let lat = lat_p.parse(data.data);
                                    let lon = lon_p.parse(data.data);
                                    let alt = alt_p.parse(data.data);
                                    Some((lat as f64 * 1e-7, lon as f64 * 1e-7, alt as f64 * 1e-3))
                                } else {
                                    None
                                };

                            if let Some((lat_deg, lon_deg, alt_m)) = coords {
                                if lat_deg.is_finite()
                                    && lon_deg.is_finite()
                                    && alt_m.is_finite()
                                    && lat_deg.abs() <= 90.0
                                    && lon_deg.abs() <= 180.0
                                    && (lat_deg != 0.0 || lon_deg != 0.0)
                                {
                                    // Find current mode from mode_changes
                                    let mode_id = mode_changes
                                        .iter()
                                        .rev()
                                        .find(|(t, _)| *t <= ts)
                                        .map(|(_, m)| *m)
                                        .unwrap_or(0);

                                    analysis.gps_track.push(TrackPoint {
                                        lat_deg,
                                        lon_deg,
                                        alt_m,
                                        timestamp_us: ts,
                                        mode_id,
                                    });
                                    last_track_ts = ts;
                                }
                            }
                        }
                    }
                }

                "vehicle_imu_status" => {
                    if let Ok(vibe_p) = data
                        .flattened_format
                        .get_field_parser::<f32>("accel_vibration_metric")
                    {
                        let v = vibe_p.parse(data.data);
                        if v.is_finite() && v >= 0.0 {
                            vibe_sum += v as f64;
                            vibe_count += 1;
                            if v > vibe_max {
                                vibe_max = v;
                            }
                        }
                    }
                }

                _ => {}
            }
        }
        SimpleCallbackResult::KeepReading
    })?;

    // --- Finalize flight modes ---
    // Close the last mode segment using the last known timestamp
    if let Some(nav) = current_nav_state {
        // Header time and first-data time are not interchangeable origins.
        let end_us = recorded_end_us.unwrap_or(mode_start_us);
        analysis.flight_modes.push(FlightModeSegment {
            mode: nav_state_name(nav).to_string(),
            mode_id: nav,
            start_us: mode_start_us,
            end_us,
            duration_s: (end_us - mode_start_us) as f64 / 1_000_000.0,
        });
    }

    // Close last VTOL segment
    if let Some(vt) = current_vehicle_type {
        let it = current_in_transition.unwrap_or(false);
        let end_us = recorded_end_us.unwrap_or(vtol_start_us);
        analysis.vtol_states.push(VtolStateSegment {
            state: vtol_state_name(vt, it).to_string(),
            start_us: vtol_start_us,
            end_us,
        });
    }

    // --- Finalize flight stats ---
    analysis.stats.total_distance_m = position.distance;
    analysis.stats.max_altitude_diff_m = position.altitude_span;
    analysis.stats.max_speed_m_s = position.speed_max;
    analysis.stats.max_horizontal_speed_m_s = position.horizontal_max;
    analysis.stats.max_speed_up_m_s = position.up_max;
    analysis.stats.max_speed_down_m_s = position.down_max;
    if position.speed_count > 0 {
        analysis.stats.avg_speed_m_s = position.speed_sum / position.speed_count as f64;
    }
    analysis.stats.max_tilt_deg = max_tilt_rad.to_degrees() as f64;
    analysis.stats.max_rotation_speed_deg_s = max_rotation_speed_rad_s.to_degrees() as f64;

    // --- Finalize battery ---
    if has_battery_data {
        if current_count > 0 {
            analysis.battery.avg_current_a = Some(current_sum / current_count as f64);
            analysis.battery.max_current_a = Some(max_current as f64);
        }
        if min_voltage < f32::MAX {
            analysis.battery.min_voltage_v = Some(min_voltage as f64);
        }
        analysis.battery.discharged_mah = last_discharged.map(|d| d as f64);
    }

    // --- Finalize GPS quality ---
    if has_gps_quality {
        if min_sats < u16::MAX {
            analysis.gps_quality.min_satellites = Some(min_sats);
        }
        if max_sats > 0 {
            analysis.gps_quality.max_satellites = Some(max_sats);
        }
        if max_hdop > 0.0 {
            analysis.gps_quality.max_hdop = Some(max_hdop);
        }
        if max_eph > 0.0 {
            analysis.gps_quality.max_eph_m = Some(max_eph);
        }
        if max_epv > 0.0 {
            analysis.gps_quality.max_epv_m = Some(max_epv);
        }
        fix_types_seen.sort();
        analysis.gps_quality.fix_types_seen = fix_types_seen;
    }

    // --- Finalize vibration ---
    if vibe_count > 0 {
        let mean = vibe_sum / vibe_count as f64;
        let max = vibe_max as f64;
        analysis.vibration.accel_vibe_mean = Some(mean);
        analysis.vibration.accel_vibe_max = Some(max);
        // Status escalates on either the mean or the peak. A short but severe
        // vibration burst (e.g. accel clipping) can leave the mean low while the
        // peak is well into critical territory, so gate on both.
        let mean_status = if mean < 4.905 {
            0
        } else if mean < 9.81 {
            1
        } else {
            2
        };
        let max_status = if max < 9.81 {
            0
        } else if max < 19.62 {
            1
        } else {
            2
        };
        analysis.vibration.status = match mean_status.max(max_status) {
            0 => "good".to_string(),
            1 => "warning".to_string(),
            _ => "critical".to_string(),
        };
    }

    // --- Finalize per-field stats ---
    for (_topic, stats_vec) in field_stats_map {
        for rs in stats_vec {
            if let Some(fs) = rs.into_field_stat() {
                analysis.field_stats.push(fs);
            }
        }
    }
    // Sort for deterministic output
    analysis
        .field_stats
        .sort_by(|a, b| a.topic.cmp(&b.topic).then_with(|| a.field.cmp(&b.field)));

    // --- Finalize diagnostics ---
    analysis.diagnostics = analyzers.into_iter().flat_map(|a| a.finish()).collect();

    Ok(analysis)
}

/// Compare current parameters against defaults to find non-default values.
/// Skips parameters starting with "RC" or "CAL_" since calibration values
/// are always device-specific.
fn compute_non_default_params(metadata: &FlightMetadata, analysis: &mut FlightAnalysis) {
    for (name, value) in &metadata.parameters {
        // Skip calibration and RC params
        if name.starts_with("RC") || name.starts_with("CAL_") {
            continue;
        }

        if let Some(default) = metadata.default_parameters.get(name) {
            let (val_f64, def_f64) = match (value, default) {
                (ParamValue::Float(v), ParamValue::Float(d)) => (*v as f64, *d as f64),
                (ParamValue::Int32(v), ParamValue::Int32(d)) => (*v as f64, *d as f64),
                // Mixed types — compare as f64
                (ParamValue::Float(v), ParamValue::Int32(d)) => (*v as f64, *d as f64),
                (ParamValue::Int32(v), ParamValue::Float(d)) => (*v as f64, *d as f64),
            };

            if (val_f64 - def_f64).abs() > f64::EPSILON {
                analysis.non_default_params.push(ParamDiff {
                    name: name.clone(),
                    value: val_f64,
                    default: def_f64,
                });
            }
        }
    }

    // Sort for deterministic output
    analysis
        .non_default_params
        .sort_by(|a, b| a.name.cmp(&b.name));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::testing::{make_data_message, MessageBuilder};
    use crate::metadata::extract_metadata;

    fn synthetic_analysis(
        header_us: u64,
        formats: &[&str],
        subscriptions: &[(u16, u8, &str)],
        records: &[(u16, Vec<u8>)],
    ) -> FlightAnalysis {
        fn message(bytes: &mut Vec<u8>, kind: u8, payload: &[u8]) {
            bytes.extend_from_slice(&(payload.len() as u16).to_le_bytes());
            bytes.push(kind);
            bytes.extend_from_slice(payload);
        }
        let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        std::fs::create_dir_all(&target).unwrap();
        let dir = tempfile::tempdir_in(target).unwrap();
        let path = dir.path().join("general-analysis.ulg");
        let mut bytes = b"ULog\x01\x12\x35\x01".to_vec();
        bytes.extend_from_slice(&header_us.to_le_bytes());
        message(&mut bytes, b'B', &[0; 40]);
        for format in formats {
            message(&mut bytes, b'F', format.as_bytes());
        }
        for (id, instance, name) in subscriptions {
            let mut payload = vec![*instance];
            payload.extend_from_slice(&id.to_le_bytes());
            payload.extend_from_slice(name.as_bytes());
            message(&mut bytes, b'A', &payload);
        }
        for (id, data) in records {
            let mut payload = id.to_le_bytes().to_vec();
            payload.extend_from_slice(data);
            message(&mut bytes, b'D', &payload);
        }
        std::fs::write(&path, bytes).unwrap();
        let path = path.to_str().unwrap();
        let metadata = extract_metadata(path).unwrap();
        analyze(path, &metadata).unwrap()
    }

    fn px4_ulog_fixture(name: &str) -> String {
        let manifest = env!("CARGO_MANIFEST_DIR");
        std::path::Path::new(manifest)
            .parent()
            .unwrap() // crates/
            .parent()
            .unwrap() // workspace root
            .join("crates/converter/tests/fixtures")
            .join(name)
            .to_string_lossy()
            .to_string()
    }

    #[test]
    fn test_analyze_sample() {
        let path = px4_ulog_fixture("sample.ulg");
        let meta = extract_metadata(&path).unwrap();
        let analysis = analyze(&path, &meta).unwrap();

        // Should have flight modes
        assert!(!analysis.flight_modes.is_empty());
        // All 678 XY samples are invalid; Z is valid, so only vertical travel
        // is retained and a three-dimensional speed cannot be reported.
        assert!((analysis.stats.total_distance_m - 0.13642866164445877).abs() < 1e-9);
        assert_eq!(analysis.stats.max_horizontal_speed_m_s, 0.0);
        assert_eq!(analysis.stats.max_speed_m_s, 0.0);
    }

    #[test]
    fn test_analyze_fixed_wing() {
        let path = px4_ulog_fixture("fixed_wing_gps.ulg");
        let meta = extract_metadata(&path).unwrap();
        let analysis = analyze(&path, &meta).unwrap();

        // Fixed wing should have GPS track
        assert!(!analysis.gps_track.is_empty());
        // Should have flight duration worth of modes
        assert!(!analysis.flight_modes.is_empty());
        // Should have GPS quality data
        assert!(analysis.gps_quality.max_satellites.is_some());
        assert_eq!(analysis.flight_modes.last().unwrap().end_us, 728_940_452);
        assert!(
            !analysis.vtol_states.is_empty(),
            "PX4 transition flag is bool"
        );
        assert_eq!(analysis.vtol_states.last().unwrap().end_us, 728_940_452);
    }

    #[test]
    fn test_gps_track_new_field_names() {
        let records = [1_000_000, 2_000_000, 1_500_000, 3_000_000]
            .into_iter()
            .map(|ts| {
                let (_, bytes) = MessageBuilder::new("vehicle_gps_position")
                    .timestamp(ts)
                    .field_f64("latitude_deg", 47.397742)
                    .field_f64("longitude_deg", 8.545594)
                    .field_f64("altitude_msl_m", 488.25)
                    .field_u8("fix_type", 3)
                    .field_u8("satellites_used", 12)
                    .build();
                (1, bytes)
            })
            .collect::<Vec<_>>();
        let analysis = synthetic_analysis(
            1,
            &["vehicle_gps_position:uint64_t timestamp;double latitude_deg;double longitude_deg;double altitude_msl_m;uint8_t fix_type;uint8_t satellites_used;"],
            &[(1, 0, "vehicle_gps_position")],
            &records,
        );
        assert_eq!(analysis.gps_track.len(), 3);
        for (i, pt) in analysis.gps_track.iter().enumerate() {
            assert_eq!(pt.lat_deg, 47.397742);
            assert_eq!(pt.lon_deg, 8.545594);
            assert_eq!(pt.alt_m, 488.25);
            assert_eq!(pt.timestamp_us, (i as u64 + 1) * 1_000_000);
        }
    }

    #[test]
    fn timelines_use_recorded_end_and_boolean_or_legacy_transition_flag() {
        for flag_type in ["bool", "uint8_t"] {
            let status = format!("vehicle_status:uint64_t timestamp;uint8_t nav_state;uint8_t vehicle_type;{flag_type} in_transition_mode;");
            let mut records = Vec::new();
            for (ts, nav, vt, transition) in [
                (1_000_000_u64, 0, 1, 0),
                (2_000_000, 3, 1, 1),
                (1_500_000, 0, 1, 0), // stale status cannot reopen an earlier mode
                (3_000_000, 3, 2, 0),
            ] {
                let mut bytes = ts.to_le_bytes().to_vec();
                bytes.extend_from_slice(&[nav, vt, transition]);
                records.push((1, bytes));
            }
            records.push((2, 5_000_000_u64.to_le_bytes().to_vec()));
            records.push((2, 4_000_000_u64.to_le_bytes().to_vec()));
            let result = synthetic_analysis(
                120_000_000,
                &[&status, "other:uint64_t timestamp;"],
                &[(1, 0, "vehicle_status"), (2, 1, "other")],
                &records,
            );
            assert_eq!(result.flight_modes.len(), 2);
            assert_eq!(result.flight_modes[0].duration_s, 1.0);
            assert_eq!(result.flight_modes[1].duration_s, 3.0);
            assert_eq!(result.flight_modes[1].end_us, 5_000_000);
            assert_eq!(
                result
                    .vtol_states
                    .iter()
                    .map(|s| s.state.as_str())
                    .collect::<Vec<_>>(),
                ["MC", "Transition", "FW"]
            );
            assert_eq!(result.vtol_states[2].end_us, 5_000_000);
        }
    }

    fn position_message(
        ts: u64,
        xyz: [f32; 3],
        velocity: [f32; 3],
        validity: Option<[u8; 4]>,
        counters: [u8; 2],
    ) -> (px4_ulog::stream_parser::model::FlattenedFormat, Vec<u8>) {
        let mut message = MessageBuilder::new("vehicle_local_position")
            .timestamp(ts)
            .field_f32("x", xyz[0])
            .field_f32("y", xyz[1])
            .field_f32("z", xyz[2])
            .field_f32("vx", velocity[0])
            .field_f32("vy", velocity[1])
            .field_f32("vz", velocity[2])
            .field_u8("xy_reset_counter", counters[0])
            .field_u8("z_reset_counter", counters[1]);
        if let Some(validity) = validity {
            for (field, valid) in ["xy_valid", "z_valid", "v_xy_valid", "v_z_valid"]
                .into_iter()
                .zip(validity)
            {
                message = message.field_u8(field, valid);
            }
        }
        message.build()
    }

    #[test]
    fn position_valid_components_and_legacy_schemas_preserve_motion() {
        for validity in [None, Some([1; 4])] {
            let mut stats = PositionStats::default();
            for (ts, xyz) in [(1, [0.0; 3]), (2, [3.0, 4.0, -12.0])] {
                let (format, bytes) =
                    position_message(ts, xyz, [3.0, 4.0, -12.0], validity, [0; 2]);
                stats.update(&make_data_message(&format, &bytes), Some(ts));
            }
            assert_eq!(stats.distance, 13.0);
            assert_eq!(stats.altitude_span, 12.0);
            assert_eq!(stats.horizontal_max, 5.0);
            assert_eq!(stats.up_max, 12.0);
            assert_eq!(stats.speed_max, 13.0);
            assert_eq!(stats.speed_sum / stats.speed_count as f64, 13.0);
        }
        for (validity, expected_distance, horizontal, down) in [
            ([1, 0, 1, 0], 5.0, 5.0, 0.0),
            ([0, 1, 0, 1], 12.0, 0.0, 12.0),
            ([0; 4], 0.0, 0.0, 0.0),
            ([2; 4], 0.0, 0.0, 0.0), // invalid uint8 boolean encoding
        ] {
            let mut stats = PositionStats::default();
            for (ts, xyz) in [(1, [0.0; 3]), (2, [3.0, 4.0, 12.0])] {
                let (format, bytes) =
                    position_message(ts, xyz, [3.0, 4.0, 12.0], Some(validity), [0; 2]);
                stats.update(&make_data_message(&format, &bytes), Some(ts));
            }
            assert_eq!(stats.distance, expected_distance);
            assert_eq!(stats.horizontal_max, horizontal);
            assert_eq!(stats.down_max, down);
            assert_eq!(
                stats.speed_count, 0,
                "3D speed requires both velocity components"
            );
        }
    }

    #[test]
    fn position_invalid_intervals_and_resets_are_not_motion() {
        let mut stats = PositionStats::default();
        for (ts, x, z, validity, resets) in [
            (1, 0.0, 0.0, [1; 4], [255; 2]),
            (2, 100.0, 100.0, [1; 4], [0; 2]), // reset counters wrap
            (3, 103.0, 104.0, [1; 4], [0; 2]), // real 5m movement
            (4, 1000.0, 1000.0, [0; 4], [0; 2]),
            (5, 2000.0, 2000.0, [1; 4], [0; 2]), // no bridge over invalid data
            (5, 9999.0, 9999.0, [1; 4], [0; 2]), // duplicate ignored
            (2, 9999.0, 9999.0, [1; 4], [0; 2]), // stale sample ignored
            (6, f32::NAN, f32::NAN, [1; 4], [0; 2]),
            (7, 3000.0, 3000.0, [1; 4], [0; 2]),
        ] {
            let (format, bytes) =
                position_message(ts, [x, 0.0, z], [0.0; 3], Some(validity), resets);
            stats.update(&make_data_message(&format, &bytes), Some(ts));
        }
        assert_eq!(stats.distance, 5.0);
        assert_eq!(stats.altitude_span, 4.0);
    }

    #[test]
    fn stationary_estimator_reset_does_not_add_a_hundred_meters() {
        let records = [(1_000_000, 0.0, 0), (2_000_000, 100.0, 1)]
            .into_iter()
            .map(|(ts, x, reset)| {
                let mut bytes =
                    position_message(ts, [x, 0.0, 0.0], [0.0; 3], Some([1; 4]), [reset, 0]).1;
                bytes.extend_from_slice(&f32::to_le_bytes(x));
                bytes.extend_from_slice(&0.0_f32.to_le_bytes());
                (1, bytes)
            })
            .collect::<Vec<_>>();
        let result = synthetic_analysis(
            1,
            &["vehicle_local_position:uint64_t timestamp;float x;float y;float z;float vx;float vy;float vz;uint8_t xy_reset_counter;uint8_t z_reset_counter;bool xy_valid;bool z_valid;bool v_xy_valid;bool v_z_valid;float[2] delta_xy;"],
            &[(1, 0, "vehicle_local_position")],
            &records,
        );
        assert_eq!(result.stats.total_distance_m, 0.0);
        assert_eq!(result.stats.max_altitude_diff_m, 0.0);
    }

    #[test]
    fn instance_zero_is_not_pooled_with_other_estimators_or_imus() {
        let mut records = Vec::new();
        for ts in 1..=3 {
            for (id, x) in [(1, ts as f32), (2, 1000.0)] {
                records.push((
                    id,
                    position_message(ts, [x, 0.0, 0.0], [0.0; 3], Some([1; 4]), [0; 2]).1,
                ));
            }
            for (id, value) in [(3, 1.0), (4, 20.0), (5, 30.0)] {
                let mut bytes = (ts * 1_000_000).to_le_bytes().to_vec();
                bytes.extend_from_slice(&f32::to_le_bytes(value));
                records.push((id, bytes));
            }
        }
        let result = synthetic_analysis(
            1,
            &[
                "vehicle_local_position:uint64_t timestamp;float x;float y;float z;float vx;float vy;float vz;uint8_t xy_reset_counter;uint8_t z_reset_counter;bool xy_valid;bool z_valid;bool v_xy_valid;bool v_z_valid;",
                "vehicle_imu_status:uint64_t timestamp;float accel_vibration_metric;",
            ],
            &[(1, 0, "vehicle_local_position"), (2, 1, "vehicle_local_position"), (3, 0, "vehicle_imu_status"), (4, 1, "vehicle_imu_status"), (5, 2, "vehicle_imu_status")],
            &records,
        );
        assert_eq!(result.stats.total_distance_m, 2.0);
        assert_eq!(result.vibration.accel_vibe_mean, Some(1.0));
        let field = result
            .field_stats
            .iter()
            .find(|s| s.topic == "vehicle_imu_status")
            .unwrap();
        assert_eq!(field.count, 3);
        assert_eq!(field.mean, 1.0);
    }

    #[test]
    fn test_field_stats_populated() {
        let path = px4_ulog_fixture("sample.ulg");
        let meta = extract_metadata(&path).unwrap();
        let analysis = analyze(&path, &meta).unwrap();

        // Should have field stats for multiple topics
        assert!(
            !analysis.field_stats.is_empty(),
            "field_stats should not be empty"
        );

        // Check we have stats from multiple topics
        let topics: std::collections::HashSet<&str> = analysis
            .field_stats
            .iter()
            .map(|fs| fs.field.as_str())
            .collect();
        assert!(topics.len() > 1, "should have stats for multiple fields");

        // All stats should have valid values
        for fs in &analysis.field_stats {
            assert!(
                fs.count > 0,
                "count should be > 0 for {}.{}",
                fs.topic,
                fs.field
            );
            assert!(
                fs.min <= fs.max,
                "min should be <= max for {}.{}",
                fs.topic,
                fs.field
            );
            assert!(
                fs.mean >= fs.min && fs.mean <= fs.max,
                "mean should be between min and max for {}.{}",
                fs.topic,
                fs.field
            );
        }
    }

    #[test]
    fn test_field_stats_fixed_wing() {
        let path = px4_ulog_fixture("fixed_wing_gps.ulg");
        let meta = extract_metadata(&path).unwrap();
        let analysis = analyze(&path, &meta).unwrap();

        // Should have field stats
        assert!(
            !analysis.field_stats.is_empty(),
            "field_stats should not be empty for fixed_wing_gps"
        );

        // Check that known topics are represented
        let topic_names: std::collections::HashSet<&str> = analysis
            .field_stats
            .iter()
            .map(|fs| fs.topic.as_str())
            .collect();
        assert!(
            topic_names.contains("vehicle_attitude"),
            "should have vehicle_attitude stats"
        );
        let imu_fields = analysis
            .field_stats
            .iter()
            .filter(|fs| fs.topic == "vehicle_imu_status")
            .collect::<Vec<_>>();
        assert!(!imu_fields.is_empty());
        assert!(
            imu_fields.iter().all(|fs| fs.count == 605),
            "three separate 605-sample instances must not become 1815 samples"
        );

        // Verify at most MAX_FIELDS_PER_TOPIC fields per topic
        let mut topic_field_counts: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for fs in &analysis.field_stats {
            *topic_field_counts.entry(fs.topic.as_str()).or_insert(0) += 1;
        }
        for (topic, count) in &topic_field_counts {
            assert!(
                *count <= 10,
                "topic {} has {} fields, expected <= 10",
                topic,
                count
            );
        }
    }

    #[test]
    fn test_diagnostics_no_false_positives() {
        let path = px4_ulog_fixture("sample.ulg");
        let meta = extract_metadata(&path).unwrap();
        let analysis = analyze(&path, &meta).unwrap();
        assert!(
            analysis.diagnostics.is_empty(),
            "Normal flight should produce no diagnostics, got: {:?}",
            analysis.diagnostics
        );
    }
}
