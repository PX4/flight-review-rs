//! Diagnostic analyzers for automated flight anomaly detection.
//!
//! Each analyzer implements the [`Analyzer`] trait, declares the ULog topics it
//! needs, receives messages during the existing `analyze()` streaming pass, and
//! emits [`Diagnostic`] structs with severity, timestamps, and typed evidence.
//!
//! # Adding a new analyzer
//!
//! 1. Create `crates/converter/src/diagnostics/your_analyzer.rs`
//! 2. Add a new variant to [`Evidence`] for your diagnostic type
//! 3. Implement the [`Analyzer`] trait
//! 4. Register it in [`create_analyzers()`]
//! 5. Add tests following the required pattern in [`testing`]
//! 6. Run `cargo bench` and include results in your PR

use px4_ulog::stream_parser::model::{DataMessage, ParseableFieldType};
use serde::{Deserialize, Serialize};

pub mod battery_brownout;
pub mod ekf_failure;
pub mod gps_interference;
pub mod motor_failure;
pub mod rc_loss;
#[cfg(test)]
pub mod testing;

/// Current analysis version. Bump when the analyzer set changes to trigger
/// reprocessing of historical logs.
pub const ANALYSIS_VERSION: u32 = 1;

/// Severity of a detected anomaly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational — notable but not a problem.
    Info,
    /// Warning — potential issue, worth investigating.
    Warning,
    /// Critical — likely hardware failure or dangerous condition.
    Critical,
}

/// Typed evidence for each diagnostic kind.
///
/// Every analyzer returns a specific variant — not a freeform map. The frontend
/// matches on `evidence.type` and renders structured UI for each diagnostic.
/// Adding a new analyzer means adding a new variant here; changing an existing
/// variant's fields is a breaking change requiring a version bump and backfill.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Evidence {
    MotorFailure {
        motor_index: u8,
        pwm_value: f32,
        /// "drop_to_zero" or "locked_at_max"
        failure_mode: String,
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
}

/// A single detected anomaly with typed evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Machine-readable identifier, e.g. "motor_failure", "gps_interference".
    pub id: String,
    /// Human-readable summary.
    pub summary: String,
    /// Severity classification.
    pub severity: Severity,
    /// Timestamp (microseconds) where the anomaly was first detected.
    pub timestamp_us: u64,
    /// Optional end timestamp if the anomaly spans a window.
    pub end_timestamp_us: Option<u64>,
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

/// Create all Phase 1 diagnostic analyzers.
pub fn create_analyzers() -> Vec<Box<dyn Analyzer>> {
    vec![
        Box::new(motor_failure::MotorFailureAnalyzer::new()),
        Box::new(gps_interference::GpsInterferenceAnalyzer::new()),
        Box::new(battery_brownout::BatteryBrownoutAnalyzer::new()),
        Box::new(ekf_failure::EkfFailureAnalyzer::new()),
        Box::new(rc_loss::RcLossAnalyzer::new()),
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
