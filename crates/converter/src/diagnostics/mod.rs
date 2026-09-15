//! Diagnostic analyzers for automated flight anomaly detection.
//!
//! Each analyzer implements the [`Analyzer`] trait, declares the ULog topics it
//! needs, receives messages during the existing `analyze()` streaming pass, and
//! emits [`Diagnostic`] structs with severity, timestamps, typed evidence, and
//! an [`OutputDescriptor`] that describes how to interpret the evidence fields.
//!
//! # Adding a new analyzer
//!
//! 1. Create `crates/converter/src/diagnostics/your_analyzer.rs`
//! 2. Add a new variant to [`Evidence`] for your diagnostic type
//! 3. Implement the [`Analyzer`] trait (including `output_descriptor()`)
//! 4. Register it in [`create_analyzers()`]
//! 5. Add tests following the required pattern in [`testing`]

use px4_ulog::stream_parser::model::{DataMessage, FlattenedFormat, MultiId, ParseableFieldType};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

pub mod battery_brownout;
pub mod ekf_failure;
pub mod ekf_selector_whipsaw;
pub mod gps_interference;
pub mod motor_failure;
pub mod rc_loss;
pub mod tecs_nonfinite_pitch;
#[cfg(test)]
pub mod testing;

/// Current analysis version. Bump when the analyzer set changes to trigger
/// reprocessing of historical logs.
pub const ANALYSIS_VERSION: u32 = 4;

/// Whether a diagnostic marks an instant or spans a time window.
///
/// `end_timestamp_us` lives on `Region` — a point cannot have an end,
/// and a region must. Invalid states are unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyKind {
    /// Single point in time (e.g., battery brownout, motor drop-to-zero).
    Point,
    /// Spans a time window (e.g., EKF failure, RC loss).
    Region {
        /// End timestamp (microseconds) of the anomaly window.
        end_timestamp_us: u64,
    },
}

/// Where on a plot this specific anomaly should be anchored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlotAnchor {
    pub topic: String,
    pub field: String,
    /// ULog multi_id. Omitted for the default instance (zero).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<u8>,
}

impl PlotAnchor {
    pub fn new(topic: &str, field: &str) -> Self {
        Self {
            topic: topic.to_string(),
            field: field.to_string(),
            instance: None,
        }
    }
}

/// Typed semantic for an evidence field value.
///
/// Used instead of free-form unit/format strings so invalid descriptors
/// are caught at compile time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldUnit {
    Volts,
    Amps,
    Meters,
    Microseconds,
    Milliseconds,
    Pwm,
    /// Driver-specific actuator command units, not measured motor speed.
    ActuatorOutput,
    Ratio,
    Count,
    /// Free-form string field (flight mode, innovation name, etc.).
    Label,
}

/// Descriptor for a single evidence field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDescriptor {
    pub name: String,
    pub unit: FieldUnit,
}

/// Describes how to interpret a diagnostic's evidence fields.
///
/// Built via typed constructors — not hand-written JSON.
/// Embedded on each [`Diagnostic`], self-contained and pre-joined.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputDescriptor {
    pub fields: Vec<FieldDescriptor>,
}

impl OutputDescriptor {
    pub fn new() -> Self {
        Self { fields: vec![] }
    }

    /// Declare an evidence field with a typed unit.
    pub fn field(mut self, name: &str, unit: FieldUnit) -> Self {
        self.fields.push(FieldDescriptor {
            name: name.to_string(),
            unit,
        });
        self
    }
}

impl Default for OutputDescriptor {
    fn default() -> Self {
        Self::new()
    }
}

/// Severity of a detected anomaly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational — notable but not a problem.
    Info,
    /// Warning — potential issue, worth investigating.
    Warning,
    /// Critical — a severe observed anomaly, not proof of a physical cause.
    Critical,
}

/// Motor failure mode — typed discriminant replacing free-form string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotorFailureMode {
    DropToZero,
    /// Legacy output. No longer inferred without driver-specific output limits.
    LockedAtMax,
}

/// Which TECS field first went non-finite — typed discriminant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TecsNonfinitePitchField {
    /// `tecs_status.pitch_integ` — the integrator, where the NaN latches first.
    PitchInteg,
    /// `tecs_status.pitch_sp_rad` — the pitch setpoint the NaN propagates into.
    PitchSpRad,
}

/// Typed evidence for each diagnostic kind.
///
/// Every analyzer returns a specific variant — not a freeform map.
/// Adding a new analyzer means adding a new variant here; changing an
/// existing variant's fields is a breaking change requiring a version
/// bump and snapshot update.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Evidence {
    MotorFailure {
        /// Legacy name: this is an actuator output channel, not a motor identity.
        motor_index: u8,
        /// Legacy name: value is in the output driver's natural units.
        pwm_value: f32,
        mode: MotorFailureMode,
        flight_mode: String,
    },
    GpsInterference {
        eph_m: f32,
        epv_m: f32,
        num_satellites: u16,
        noise_level: Option<f32>,
    },
    BatteryBrownout {
        voltage_v: f32,
        critical_threshold_v: f32,
        current_a: Option<f32>,
    },
    EkfFailure {
        /// Which innovation failed (e.g. "velocity", "position", "height")
        innovation: String,
        test_ratio: f32,
        threshold: f32,
    },
    RcLoss {
        last_signal_timestamp_us: u64,
        signal_lost_duration_ms: u64,
    },
    EkfSelectorWhipsaw {
        /// Number of instance switches in the detection window.
        switch_count: u32,
        /// Duration of the detection window (milliseconds).
        window_duration_ms: u64,
        /// Average interval between observed switch updates; unavailable when
        /// counter jumps hide intermediate switch times.
        avg_switch_interval_ms: Option<f64>,
        /// True if the selector switched to an instance with a high
        /// combined_test_ratio (indicating switching to a degraded instance).
        /// This is the #27013 signature.
        switched_to_degraded: bool,
        /// combined_test_ratio of the primary instance at detection time.
        primary_instance_test_ratio: Option<f32>,
    },
    TecsNonfinitePitch {
        /// Which field first went non-finite (integrator or setpoint).
        field: TecsNonfinitePitchField,
        /// True if `throttle_integ` was also non-finite at the same sample —
        /// a sibling-channel sanity check that the whole TECS state corrupted.
        throttle_integ_nonfinite: bool,
    },
}

/// A single detected anomaly with typed evidence and output descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Machine-readable identifier, e.g. "motor_failure", "gps_interference".
    pub id: String,
    /// Human-readable summary.
    pub summary: String,
    /// Severity classification.
    pub severity: Severity,
    /// Whether this is a point-in-time or a region spanning a window.
    /// For regions, carries `end_timestamp_us` inside the variant.
    pub kind: AnomalyKind,
    /// Timestamp (microseconds) where the anomaly was first detected.
    pub timestamp_us: u64,
    /// Where on a plot this anomaly should be anchored: (topic, field).
    pub anchor: PlotAnchor,
    /// Describes how to interpret the evidence fields (types, units).
    pub descriptor: OutputDescriptor,
    /// Typed, structured evidence specific to this diagnostic.
    pub evidence: Evidence,
}

/// Trait that all diagnostic analyzers implement.
///
/// Analyzers are created by [`create_analyzers()`], receive messages via
/// [`on_message()`](Analyzer::on_message) during the streaming pass in
/// `analyze()`, and emit diagnostics via [`finish()`](Analyzer::finish).
pub trait Analyzer {
    /// Machine-readable identifier (e.g. "motor_failure").
    fn id(&self) -> &str;

    /// Short human-readable description.
    fn description(&self) -> &str;

    /// Which ULog topics this analyzer needs.
    /// The `analyze()` callback will only dispatch messages for these topics.
    fn required_topics(&self) -> &[&str];

    /// Called once per data message for a subscribed topic.
    fn on_message(&mut self, data: &DataMessage);

    /// Called after the streaming pass completes. Return any detected anomalies.
    fn finish(self: Box<Self>) -> Vec<Diagnostic>;

    /// Describes this analyzer's output shape — field names and typed units.
    fn output_descriptor(&self) -> OutputDescriptor;
}

/// Parse a typed field from a DataMessage, returning None if the field is
/// missing or has the wrong type. All analyzers should use this instead of
/// calling get_field_parser directly.
pub fn parse_field<T: ParseableFieldType>(data: &DataMessage, name: &str) -> Option<T> {
    data.flattened_format
        .get_field_parser::<T>(name)
        .ok()
        .map(|p| p.parse(data.data))
}

/// PX4 bool fields are distinct from uint8 fields in the ULog parser. Some
/// historical schemas used uint8; do not treat an absent field as false.
pub fn parse_bool(data: &DataMessage, name: &str) -> Option<bool> {
    parse_field::<bool>(data, name).or_else(|| {
        parse_field::<u8>(data, name).and_then(|value| match value {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        })
    })
}

pub fn timestamp(data: &DataMessage) -> Option<u64> {
    data.flattened_format
        .timestamp_field
        .as_ref()
        .map(|field| field.parse_timestamp(data.data))
}

struct VehicleStatusSample {
    timestamp: u64,
    format: FlattenedFormat,
    bytes: Vec<u8>,
}

#[derive(Default)]
struct InstanceState<A> {
    analyzer: A,
    last_status: Option<u64>,
    last_measurement: Option<u64>,
}

impl<A: Analyzer> InstanceState<A> {
    fn apply_status_through(&mut self, history: &VecDeque<VehicleStatusSample>, timestamp: u64) {
        let start = history.partition_point(|status| {
            self.last_status
                .is_some_and(|previous| status.timestamp <= previous)
        });
        for status in history.iter().skip(start) {
            if status.timestamp > timestamp {
                break;
            }
            self.analyzer.on_message(&DataMessage {
                msg_id: 0,
                multi_id: MultiId::new(0),
                flattened_format: &status.format,
                data: &status.bytes,
            });
            self.last_status = Some(status.timestamp);
        }
    }
}

/// Isolate sensor state and associate it with primary vehicle status by time,
/// not arrival order. Status updates newer than a measurement are deferred.
struct PerInstance<A> {
    prototype: A,
    instances: BTreeMap<u8, InstanceState<A>>,
    vehicle_status: VecDeque<VehicleStatusSample>,
}

impl<A: Analyzer + Default> Default for PerInstance<A> {
    fn default() -> Self {
        Self {
            prototype: A::default(),
            instances: BTreeMap::new(),
            vehicle_status: VecDeque::new(),
        }
    }
}

impl<A: Analyzer + Default> Analyzer for PerInstance<A> {
    fn id(&self) -> &str {
        self.prototype.id()
    }
    fn description(&self) -> &str {
        self.prototype.description()
    }
    fn required_topics(&self) -> &[&str] {
        self.prototype.required_topics()
    }
    fn output_descriptor(&self) -> OutputDescriptor {
        self.prototype.output_descriptor()
    }

    fn on_message(&mut self, data: &DataMessage) {
        let Some(ts) = timestamp(data) else { return };
        if data.flattened_format.message_name == "vehicle_status" {
            if data.multi_id.value() != 0 {
                return;
            }
            let index = self
                .vehicle_status
                .partition_point(|status| status.timestamp < ts);
            if self
                .vehicle_status
                .get(index)
                .is_some_and(|status| status.timestamp == ts)
            {
                return;
            }
            self.vehicle_status.insert(
                index,
                VehicleStatusSample {
                    timestamp: ts,
                    format: data.flattened_format.clone(),
                    bytes: data.data.to_vec(),
                },
            );
            // Bounded history for delayed/new sources. Measurements older than
            // retained state cannot safely be classified as armed.
            if self.vehicle_status.len() > 128 {
                self.vehicle_status.pop_front();
            }
            return;
        }
        if self.required_topics().contains(&"vehicle_status")
            && self
                .vehicle_status
                .front()
                .is_none_or(|oldest| ts < oldest.timestamp)
        {
            return;
        }
        let instance = self.instances.entry(data.multi_id.value()).or_default();
        if instance
            .last_measurement
            .is_some_and(|previous| ts <= previous)
        {
            return;
        }
        instance.apply_status_through(&self.vehicle_status, ts);
        instance.last_measurement = Some(ts);
        instance.analyzer.on_message(data);
    }

    fn finish(self: Box<Self>) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (instance, mut state) in self.instances {
            state.apply_status_through(&self.vehicle_status, u64::MAX);
            for mut diagnostic in Box::new(state.analyzer).finish() {
                diagnostic.anchor.instance = (instance != 0).then_some(instance);
                diagnostics.push(diagnostic);
            }
        }
        diagnostics.sort_by_key(|d| d.timestamp_us);
        diagnostics
    }
}

/// Create all diagnostic analyzers.
pub fn create_analyzers() -> Vec<Box<dyn Analyzer>> {
    vec![
        Box::new(PerInstance::<motor_failure::MotorFailureAnalyzer>::default()),
        Box::new(PerInstance::<gps_interference::GpsInterferenceAnalyzer>::default()),
        Box::new(PerInstance::<battery_brownout::BatteryBrownoutAnalyzer>::default()),
        Box::new(PerInstance::<ekf_failure::EkfFailureAnalyzer>::default()),
        Box::new(PerInstance::<rc_loss::RcLossAnalyzer>::default()),
        Box::new(PerInstance::<
            ekf_selector_whipsaw::EkfSelectorWhipsawAnalyzer,
        >::default()),
        Box::new(PerInstance::<
            tecs_nonfinite_pitch::TecsNonfinitePitchAnalyzer,
        >::default()),
    ]
}

/// Create only the analyzers whose IDs are in the given list.
/// Returns an error string if any ID is unrecognized.
pub fn create_analyzers_filtered(ids: &[String]) -> Result<Vec<Box<dyn Analyzer>>, String> {
    let all = create_analyzers();
    let mut selected = Vec::new();
    for id in ids {
        let found = all.iter().any(|a| a.id() == id.as_str());
        if !found {
            let valid: Vec<&str> = all.iter().map(|a| a.id()).collect();
            return Err(format!(
                "unknown analyzer '{}'. valid: {}",
                id,
                valid.join(", ")
            ));
        }
    }
    for a in all {
        if ids.iter().any(|id| id == a.id()) {
            selected.push(a);
        }
    }
    Ok(selected)
}
