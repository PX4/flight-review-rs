//! Extract flight analysis data from ULog files.
//!
//! Performs a streaming pass through the ULog file, extracting typed field
//! values from specific topics to produce flight statistics, mode timelines,
//! battery summaries, GPS quality metrics, vibration data, and GPS tracks.

use crate::metadata::{FlightMetadata, ParamValue};
use px4_ulog::stream_parser::file_reader::{
    read_file_with_simple_callback, Message, SimpleCallbackResult,
};
use px4_ulog::stream_parser::model::FlattenedFieldType;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::collections::HashMap;

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
    /// Per-topic-field statistics (min, max, mean)
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
    pub total_distance_m: f64,
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
                if off + 4 > data.len() { return; }
                f32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as f64
            }
            FlattenedFieldType::Double => {
                if off + 8 > data.len() { return; }
                f64::from_le_bytes(data[off..off + 8].try_into().unwrap())
            }
            FlattenedFieldType::Int32 => {
                if off + 4 > data.len() { return; }
                i32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as f64
            }
            FlattenedFieldType::UInt32 => {
                if off + 4 > data.len() { return; }
                u32::from_le_bytes(data[off..off + 4].try_into().unwrap()) as f64
            }
            FlattenedFieldType::Int16 => {
                if off + 2 > data.len() { return; }
                i16::from_le_bytes(data[off..off + 2].try_into().unwrap()) as f64
            }
            FlattenedFieldType::UInt16 => {
                if off + 2 > data.len() { return; }
                u16::from_le_bytes(data[off..off + 2].try_into().unwrap()) as f64
            }
            FlattenedFieldType::Int8 => {
                data[off] as i8 as f64
            }
            FlattenedFieldType::UInt8 => {
                data[off] as f64
            }
            FlattenedFieldType::Int64 => {
                if off + 8 > data.len() { return; }
                i64::from_le_bytes(data[off..off + 8].try_into().unwrap()) as f64
            }
            FlattenedFieldType::UInt64 => {
                if off + 8 > data.len() { return; }
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

/// Analyze a ULog file, extracting flight statistics, mode timeline, battery
/// summary, GPS quality, vibration data, and GPS track.
///
/// This performs a second streaming pass through the file (the first pass is
/// done by `extract_metadata`). Missing topics are handled gracefully —
/// the corresponding fields in the result will be empty/default.
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
    let mut prev_pos: Option<(f32, f32, f32)> = None;
    let mut total_distance: f64 = 0.0;
    let mut min_z: f32 = f32::MAX;
    let mut max_z: f32 = f32::MIN;
    let mut max_speed_3d: f32 = 0.0f32;
    let mut max_speed_h: f32 = 0.0f32;
    let mut max_speed_up: f32 = 0.0f32;
    let mut max_speed_down: f32 = 0.0f32;
    let mut speed_sum: f64 = 0.0;
    let mut speed_count: u64 = 0;

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
    // Key is (message_name, msg_id) to handle multi-instance topics.
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
                            let in_trans = data
                                .flattened_format
                                .get_field_parser::<u8>("in_transition_mode")
                                .ok()
                                .map(|p| p.parse(data.data) != 0);

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
                        if let (Ok(x_p), Ok(y_p), Ok(z_p), Ok(vx_p), Ok(vy_p), Ok(vz_p)) = (
                            data.flattened_format.get_field_parser::<f32>("x"),
                            data.flattened_format.get_field_parser::<f32>("y"),
                            data.flattened_format.get_field_parser::<f32>("z"),
                            data.flattened_format.get_field_parser::<f32>("vx"),
                            data.flattened_format.get_field_parser::<f32>("vy"),
                            data.flattened_format.get_field_parser::<f32>("vz"),
                        ) {
                            let x = x_p.parse(data.data);
                            let y = y_p.parse(data.data);
                            let z = z_p.parse(data.data);
                            let vx = vx_p.parse(data.data);
                            let vy = vy_p.parse(data.data);
                            let vz = vz_p.parse(data.data);

                            // Skip NaN values
                            if x.is_finite() && y.is_finite() && z.is_finite() {
                                // Distance accumulation
                                if let Some((px, py, pz)) = prev_pos {
                                    let dx = (x - px) as f64;
                                    let dy = (y - py) as f64;
                                    let dz = (z - pz) as f64;
                                    total_distance += (dx * dx + dy * dy + dz * dz).sqrt();
                                }
                                prev_pos = Some((x, y, z));

                                // Altitude tracking (NED: z negative = up)
                                if z < min_z {
                                    min_z = z;
                                }
                                if z > max_z {
                                    max_z = z;
                                }
                            }

                            if vx.is_finite() && vy.is_finite() && vz.is_finite() {
                                let speed_3d =
                                    ((vx * vx + vy * vy + vz * vz) as f64).sqrt() as f32;
                                let speed_h = ((vx * vx + vy * vy) as f64).sqrt() as f32;

                                if speed_3d > max_speed_3d {
                                    max_speed_3d = speed_3d;
                                }
                                if speed_h > max_speed_h {
                                    max_speed_h = speed_h;
                                }
                                // NED: negative vz = up
                                if -vz > max_speed_up {
                                    max_speed_up = -vz;
                                }
                                if vz > max_speed_down {
                                    max_speed_down = vz;
                                }

                                speed_sum += speed_3d as f64;
                                speed_count += 1;
                            }
                        }
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

                            if q0.is_finite() && q1.is_finite() && q2.is_finite() && q3.is_finite()
                            {
                                // Euler angles from quaternion
                                let roll = (2.0 * (q0 * q1 + q2 * q3))
                                    .atan2(1.0 - 2.0 * (q1 * q1 + q2 * q2));
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
                                let rot_speed =
                                    ((wx * wx + wy * wy + wz * wz) as f64).sqrt() as f32;
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
                            if ft > 2 && (ts - last_track_ts) >= 1_000_000 {
                                // Try new field names (f64 degrees) first, fall back to legacy (i32 raw)
                                let coords: Option<(f64, f64, f64)> =
                                    if let (Ok(lat_p), Ok(lon_p), Ok(alt_p)) = (
                                        data.flattened_format.get_field_parser::<f64>("latitude_deg"),
                                        data.flattened_format.get_field_parser::<f64>("longitude_deg"),
                                        data.flattened_format.get_field_parser::<f64>("altitude_msl_m"),
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
                                    if lat_deg != 0.0 || lon_deg != 0.0 {
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

                // Dispatch to diagnostic analyzers
                if diagnostic_topics.contains(topic) {
                    for analyzer in analyzers.iter_mut() {
                        if analyzer.required_topics().contains(&topic) {
                            analyzer.on_message(data);
                        }
                    }
                }
            }
        SimpleCallbackResult::KeepReading
    })?;

    // --- Finalize flight modes ---
    // Close the last mode segment using the last known timestamp
    if let Some(nav) = current_nav_state {
        // Use flight_duration_s from metadata to estimate the last timestamp
        let last_ts = metadata
            .flight_duration_s
            .map(|d| metadata.start_timestamp_us + (d * 1_000_000.0) as u64)
            .unwrap_or(mode_start_us);
        let end_us = if last_ts > mode_start_us {
            last_ts
        } else {
            mode_start_us
        };
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
        let last_ts = metadata
            .flight_duration_s
            .map(|d| metadata.start_timestamp_us + (d * 1_000_000.0) as u64)
            .unwrap_or(vtol_start_us);
        let end_us = if last_ts > vtol_start_us {
            last_ts
        } else {
            vtol_start_us
        };
        analysis.vtol_states.push(VtolStateSegment {
            state: vtol_state_name(vt, it).to_string(),
            start_us: vtol_start_us,
            end_us,
        });
    }

    // --- Finalize flight stats ---
    analysis.stats.total_distance_m = total_distance;
    if min_z < f32::MAX && max_z > f32::MIN {
        analysis.stats.max_altitude_diff_m = (max_z - min_z).abs() as f64;
    }
    analysis.stats.max_speed_m_s = max_speed_3d as f64;
    analysis.stats.max_horizontal_speed_m_s = max_speed_h as f64;
    analysis.stats.max_speed_up_m_s = max_speed_up.max(0.0) as f64;
    analysis.stats.max_speed_down_m_s = max_speed_down.max(0.0) as f64;
    if speed_count > 0 {
        analysis.stats.avg_speed_m_s = speed_sum / speed_count as f64;
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
    analysis.diagnostics = analyzers
        .into_iter()
        .flat_map(|a| a.finish())
        .collect();

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
                (ParamValue::Float(v), ParamValue::Float(d)) => {
                    (*v as f64, *d as f64)
                }
                (ParamValue::Int32(v), ParamValue::Int32(d)) => {
                    (*v as f64, *d as f64)
                }
                // Mixed types — compare as f64
                (ParamValue::Float(v), ParamValue::Int32(d)) => {
                    (*v as f64, *d as f64)
                }
                (ParamValue::Int32(v), ParamValue::Float(d)) => {
                    (*v as f64, *d as f64)
                }
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
    use crate::metadata::extract_metadata;

    fn px4_ulog_fixture(name: &str) -> String {
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
    fn test_analyze_sample() {
        let path = px4_ulog_fixture("sample.ulg");
        let meta = extract_metadata(&path).unwrap();
        let analysis = analyze(&path, &meta).unwrap();

        // Should have flight modes
        assert!(!analysis.flight_modes.is_empty());
        // Should have stats
        assert!(analysis.stats.max_speed_m_s >= 0.0);
    }

    #[test]
    fn test_analyze_fixed_wing() {
        let path = px4_ulog_fixture("fixed_wing_gps.ulg");
        if !std::path::Path::new(&path).exists() {
            eprintln!("Skipping: fixed_wing_gps.ulg not available");
            return;
        }
        let meta = extract_metadata(&path).unwrap();
        let analysis = analyze(&path, &meta).unwrap();

        // Fixed wing should have GPS track
        assert!(!analysis.gps_track.is_empty());
        // Should have flight duration worth of modes
        assert!(!analysis.flight_modes.is_empty());
        // Should have GPS quality data
        assert!(analysis.gps_quality.max_satellites.is_some());
    }

    #[test]
    fn test_gps_track_new_field_names() {
        // Test with any log that has vehicle_gps_position with latitude_deg/longitude_deg fields
        // (current PX4 format). Falls back to quadrotor_local.ulg or any available fixture.
        let candidates = ["quadrotor_gps.ulg", "fixed_wing_gps.ulg"];
        let mut path = String::new();
        for name in candidates {
            let p = px4_ulog_fixture(name);
            if std::path::Path::new(&p).exists() {
                path = p;
                break;
            }
        }
        if path.is_empty() {
            eprintln!("Skipping test_gps_track_new_field_names: no GPS fixture available");
            return;
        }
        let meta = extract_metadata(&path).unwrap();
        let analysis = analyze(&path, &meta).unwrap();

        if !analysis.gps_track.is_empty() {
            // Verify track points have valid coordinates
            for pt in &analysis.gps_track {
                assert!(pt.lat_deg.abs() <= 90.0, "latitude out of range: {}", pt.lat_deg);
                assert!(pt.lon_deg.abs() <= 180.0, "longitude out of range: {}", pt.lon_deg);
                assert!(pt.alt_m.abs() < 100_000.0, "altitude out of range: {}", pt.alt_m);
                assert!(pt.timestamp_us > 0, "timestamp should be positive");
            }

            // Track should be roughly in chronological order
            for window in analysis.gps_track.windows(2) {
                assert!(
                    window[1].timestamp_us >= window[0].timestamp_us,
                    "GPS track should be chronologically ordered"
                );
            }
        }
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
        let topics: std::collections::HashSet<&str> =
            analysis.field_stats.iter().map(|fs| fs.field.as_str()).collect();
        assert!(topics.len() > 1, "should have stats for multiple fields");

        // All stats should have valid values
        for fs in &analysis.field_stats {
            assert!(fs.count > 0, "count should be > 0 for {}.{}", fs.topic, fs.field);
            assert!(fs.min <= fs.max, "min should be <= max for {}.{}", fs.topic, fs.field);
            assert!(fs.mean >= fs.min && fs.mean <= fs.max,
                "mean should be between min and max for {}.{}", fs.topic, fs.field);
        }
    }

    #[test]
    fn test_field_stats_fixed_wing() {
        let path = px4_ulog_fixture("fixed_wing_gps.ulg");
        if !std::path::Path::new(&path).exists() {
            eprintln!("Skipping: fixed_wing_gps.ulg not available");
            return;
        }
        let meta = extract_metadata(&path).unwrap();
        let analysis = analyze(&path, &meta).unwrap();

        // Should have field stats
        assert!(
            !analysis.field_stats.is_empty(),
            "field_stats should not be empty for fixed_wing_gps"
        );

        // Check that known topics are represented
        let topic_names: std::collections::HashSet<&str> =
            analysis.field_stats.iter().map(|fs| fs.topic.as_str()).collect();
        assert!(
            topic_names.contains("vehicle_attitude"),
            "should have vehicle_attitude stats"
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
