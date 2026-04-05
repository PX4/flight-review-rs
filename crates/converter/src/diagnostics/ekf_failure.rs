//! EKF failure detection analyzer.
//!
//! Monitors `estimator_status` for innovation test ratios exceeding their
//! bounds. When a test ratio stays above 1.0 for a sustained period, the
//! EKF is failing to converge and the position/velocity estimates are
//! unreliable.

use super::{parse_field, Analyzer, Diagnostic, Evidence, Severity};
use px4_ulog::stream_parser::model::DataMessage;

/// Test ratio threshold — above this the EKF innovation is failing.
const TEST_RATIO_THRESHOLD: f32 = 1.0;
/// Duration (microseconds) of sustained exceedance for Warning.
const WARNING_DURATION_US: u64 = 2_000_000;
/// Duration (microseconds) of sustained exceedance for Critical.
const CRITICAL_DURATION_US: u64 = 5_000_000;

/// Tracks sustained exceedance for a single innovation channel.
struct InnovationTracker {
    name: &'static str,
    field_name: &'static str,
    exceeded_since: Option<u64>,
    warning_fired: bool,
    critical_fired: bool,
}

impl InnovationTracker {
    fn new(name: &'static str, field_name: &'static str) -> Self {
        Self {
            name,
            field_name,
            exceeded_since: None,
            warning_fired: false,
            critical_fired: false,
        }
    }

    fn update(&mut self, ts: u64, ratio: f32, detections: &mut Vec<Diagnostic>) {
        if !ratio.is_finite() {
            return;
        }

        if ratio > TEST_RATIO_THRESHOLD {
            if self.exceeded_since.is_none() {
                self.exceeded_since = Some(ts);
            }

            let start = self.exceeded_since.unwrap();
            let duration = ts.saturating_sub(start);

            if duration >= CRITICAL_DURATION_US && !self.critical_fired {
                self.critical_fired = true;
                self.warning_fired = true; // suppress warning if critical fires
                detections.push(Diagnostic {
                    id: "ekf_failure".to_string(),
                    summary: format!(
                        "EKF {} innovation exceeded threshold for {:.1}s starting at {:.1}s",
                        self.name,
                        duration as f64 / 1_000_000.0,
                        start as f64 / 1_000_000.0,
                    ),
                    severity: Severity::Critical,
                    timestamp_us: start,
                    end_timestamp_us: Some(ts),
                    evidence: Evidence::EkfFailure {
                        innovation: self.name.to_string(),
                        test_ratio: ratio,
                        threshold: TEST_RATIO_THRESHOLD,
                    },
                });
            } else if duration >= WARNING_DURATION_US && !self.warning_fired {
                self.warning_fired = true;
                detections.push(Diagnostic {
                    id: "ekf_failure".to_string(),
                    summary: format!(
                        "EKF {} innovation exceeded threshold for {:.1}s starting at {:.1}s",
                        self.name,
                        duration as f64 / 1_000_000.0,
                        start as f64 / 1_000_000.0,
                    ),
                    severity: Severity::Warning,
                    timestamp_us: start,
                    end_timestamp_us: Some(ts),
                    evidence: Evidence::EkfFailure {
                        innovation: self.name.to_string(),
                        test_ratio: ratio,
                        threshold: TEST_RATIO_THRESHOLD,
                    },
                });
            }
        } else {
            // Ratio dropped below threshold, reset tracking
            self.exceeded_since = None;
            self.warning_fired = false;
            self.critical_fired = false;
        }
    }
}

pub struct EkfFailureAnalyzer {
    trackers: Vec<InnovationTracker>,
    detections: Vec<Diagnostic>,
}

impl Default for EkfFailureAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl EkfFailureAnalyzer {
    pub fn new() -> Self {
        Self {
            trackers: vec![
                InnovationTracker::new("velocity", "vel_test_ratio"),
                InnovationTracker::new("position", "pos_test_ratio"),
                InnovationTracker::new("height", "hgt_test_ratio"),
            ],
            detections: Vec::new(),
        }
    }
}

impl Analyzer for EkfFailureAnalyzer {
    fn id(&self) -> &str {
        "ekf_failure"
    }

    fn description(&self) -> &str {
        "Sustained EKF innovation exceedance"
    }

    fn required_topics(&self) -> &[&str] {
        &["estimator_status"]
    }

    fn on_message(&mut self, data: &DataMessage) {
        let topic = data.flattened_format.message_name.as_str();
        if topic != "estimator_status" {
            return;
        }

        let ts = data
            .flattened_format
            .timestamp_field
            .as_ref()
            .map(|tf| tf.parse_timestamp(data.data))
            .unwrap_or(0);

        for tracker in &mut self.trackers {
            if let Some(ratio) = parse_field::<f32>(data, tracker.field_name) {
                tracker.update(ts, ratio, &mut self.detections);
            }
        }
    }

    fn finish(self: Box<Self>) -> Vec<Diagnostic> {
        self.detections
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::testing::*;

    #[test]
    fn no_false_positives_sample() {
        assert_no_false_positives("sample.ulg", "ekf_failure");
    }

    #[test]
    fn detects_sustained_velocity_failure() {
        let mut analyzer = EkfFailureAnalyzer::new();

        // Feed 3 seconds of high velocity test ratio (> warning threshold)
        for i in 0..30 {
            let ts = (i as u64 + 1) * 100_000; // 100ms intervals, 3s total
            let (fmt, data) = MessageBuilder::new("estimator_status")
                .timestamp(ts)
                .field_f32("vel_test_ratio", 1.5)
                .field_f32("pos_test_ratio", 0.3)
                .field_f32("hgt_test_ratio", 0.2)
                .build();
            let dm = make_data_message(&fmt, &data);
            analyzer.on_message(&dm);
        }

        let diags = Box::new(analyzer).finish();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Warning);
        match &diags[0].evidence {
            Evidence::EkfFailure { innovation, .. } => {
                assert_eq!(innovation, "velocity");
            }
            _ => panic!("Expected EkfFailure evidence"),
        }
    }

    #[test]
    fn escalates_to_critical() {
        let mut analyzer = EkfFailureAnalyzer::new();

        // Feed 6 seconds of high test ratio (> critical threshold)
        for i in 0..60 {
            let ts = (i as u64 + 1) * 100_000;
            let (fmt, data) = MessageBuilder::new("estimator_status")
                .timestamp(ts)
                .field_f32("vel_test_ratio", 2.0)
                .field_f32("pos_test_ratio", 0.3)
                .field_f32("hgt_test_ratio", 0.2)
                .build();
            let dm = make_data_message(&fmt, &data);
            analyzer.on_message(&dm);
        }

        let diags = Box::new(analyzer).finish();
        // Should have both warning and critical (warning fires first at 2s, critical at 5s)
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].severity, Severity::Warning);
        assert_eq!(diags[1].severity, Severity::Critical);
    }

    #[test]
    fn resets_on_recovery() {
        let mut analyzer = EkfFailureAnalyzer::new();

        // 1.5s of exceedance (below warning threshold)
        for i in 0..15 {
            let ts = (i as u64 + 1) * 100_000;
            let (fmt, data) = MessageBuilder::new("estimator_status")
                .timestamp(ts)
                .field_f32("vel_test_ratio", 1.5)
                .build();
            let dm = make_data_message(&fmt, &data);
            analyzer.on_message(&dm);
        }

        // Recovery
        let (fmt, data) = MessageBuilder::new("estimator_status")
            .timestamp(2_000_000)
            .field_f32("vel_test_ratio", 0.5)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm);

        let diags = Box::new(analyzer).finish();
        assert!(diags.is_empty(), "Should not fire if ratio drops before warning duration");
    }

    #[test]
    fn handles_missing_fields() {
        let mut analyzer = EkfFailureAnalyzer::new();

        let (fmt, data) = MessageBuilder::new("estimator_status")
            .timestamp(1_000_000)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm); // must not panic

        let diags = Box::new(analyzer).finish();
        assert!(diags.is_empty());
    }

    #[test]
    fn snapshot_sample_ulg() {
        let diags = analyze_fixture_for("sample.ulg", "ekf_failure");
        insta::assert_json_snapshot!(diags);
    }
}
