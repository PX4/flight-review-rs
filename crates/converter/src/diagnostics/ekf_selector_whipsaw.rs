//! EKF2 selector whipsaw detection analyzer.
//!
//! Detects rapid instance switching in the EKF2 multi-instance selector,
//! the failure mode reported in PX4 issue #27013. When one IMU has
//! intermittent accel clipping, the `bad_acc_clipping` fault flag toggles
//! rapidly, causing the selector to whipsaw between EKF instances. If the
//! fallback instance has a diverged state (e.g. declared `cs_baro_fault`
//! and switched to GPS-only height), each switch causes an altitude step.
//!
//! Signals:
//! - `estimator_selector_status.instance_changed_count` rising by more
//!   than N switches within a time window
//! - Combined with high `combined_test_ratio` on an instance that the
//!   selector switches to (indicates switching to a degraded instance)

use super::{parse_field, Analyzer, Diagnostic, Evidence, Severity};
use px4_ulog::stream_parser::model::DataMessage;
use std::collections::VecDeque;

/// Minimum number of instance switches within the window to flag as whipsaw.
const WARNING_SWITCH_COUNT: u32 = 3;
/// Higher threshold that escalates to critical severity.
const CRITICAL_SWITCH_COUNT: u32 = 8;
/// Time window for counting switches (microseconds).
const WINDOW_SIZE_US: u64 = 10_000_000; // 10 seconds
/// Deduplication interval to avoid flooding detections (microseconds).
const DEDUP_INTERVAL_US: u64 = 30_000_000; // 30 seconds
/// Test ratio above which an instance is considered degraded.
const DEGRADED_TEST_RATIO: f32 = 1.0;

pub struct EkfSelectorWhipsawAnalyzer {
    /// Ring buffer of switch timestamps (microseconds).
    switch_times: VecDeque<u64>,
    last_instance_changed_count: Option<u32>,
    last_primary_instance: Option<u8>,
    /// Per-instance most recent combined_test_ratio.
    last_test_ratios: [f32; 9],
    /// Whether the selector ever switched to a degraded instance.
    switched_to_degraded: bool,
    last_detection_us: Option<u64>,
    /// Track whether this log even has multi-EKF (instances_available > 1).
    multi_ekf_active: bool,
    detections: Vec<Diagnostic>,
}

impl Default for EkfSelectorWhipsawAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl EkfSelectorWhipsawAnalyzer {
    pub fn new() -> Self {
        Self {
            switch_times: VecDeque::new(),
            last_instance_changed_count: None,
            last_primary_instance: None,
            last_test_ratios: [0.0; 9],
            switched_to_degraded: false,
            last_detection_us: None,
            multi_ekf_active: false,
            detections: Vec::new(),
        }
    }

    fn trim_window(&mut self, now_us: u64) {
        let cutoff = now_us.saturating_sub(WINDOW_SIZE_US);
        while let Some(&front) = self.switch_times.front() {
            if front < cutoff {
                self.switch_times.pop_front();
            } else {
                break;
            }
        }
    }
}

impl Analyzer for EkfSelectorWhipsawAnalyzer {
    fn id(&self) -> &str {
        "ekf_selector_whipsaw"
    }

    fn description(&self) -> &str {
        "EKF2 multi-instance selector whipsaw (issue #27013)"
    }

    fn required_topics(&self) -> &[&str] {
        &["estimator_selector_status"]
    }

    fn on_message(&mut self, data: &DataMessage) {
        let topic = data.flattened_format.message_name.as_str();
        if topic != "estimator_selector_status" {
            return;
        }

        let ts = data
            .flattened_format
            .timestamp_field
            .as_ref()
            .map(|tf| tf.parse_timestamp(data.data))
            .unwrap_or(0);

        // Only relevant if multi-EKF is actually active
        if let Some(available) = parse_field::<u8>(data, "instances_available") {
            if available > 1 {
                self.multi_ekf_active = true;
            }
        }

        if !self.multi_ekf_active {
            return;
        }

        // Update per-instance test ratios
        for i in 0..9 {
            let field = format!("combined_test_ratio[{i}]");
            if let Some(ratio) = parse_field::<f32>(data, &field) {
                if ratio.is_finite() {
                    self.last_test_ratios[i as usize] = ratio;
                }
            }
        }

        // Detect instance changes
        let current_count = parse_field::<u32>(data, "instance_changed_count");
        let current_primary = parse_field::<u8>(data, "primary_instance");

        if let (Some(count), Some(primary)) = (current_count, current_primary) {
            let switched = match self.last_instance_changed_count {
                Some(prev) => count > prev,
                None => false,
            };

            if switched {
                self.switch_times.push_back(ts);

                // Check if the selector switched to a degraded instance
                // (instance with high combined_test_ratio).
                let idx = primary as usize;
                if idx < 9 && self.last_test_ratios[idx] >= DEGRADED_TEST_RATIO {
                    self.switched_to_degraded = true;
                }
            }

            self.last_instance_changed_count = Some(count);
            self.last_primary_instance = Some(primary);
        }

        self.trim_window(ts);

        let switches_in_window = self.switch_times.len() as u32;

        let dedup_ok = match self.last_detection_us {
            Some(prev) => ts.saturating_sub(prev) >= DEDUP_INTERVAL_US,
            None => true,
        };

        if switches_in_window >= WARNING_SWITCH_COUNT && dedup_ok {
            self.last_detection_us = Some(ts);

            let window_start = *self.switch_times.front().unwrap_or(&ts);
            let window_duration_us = ts.saturating_sub(window_start);
            let avg_interval_ms = if switches_in_window > 1 {
                (window_duration_us / (switches_in_window as u64 - 1)) as f64 / 1_000.0
            } else {
                0.0
            };

            let severity = if switches_in_window >= CRITICAL_SWITCH_COUNT
                || self.switched_to_degraded
            {
                Severity::Critical
            } else {
                Severity::Warning
            };

            let primary_test_ratio = self
                .last_primary_instance
                .map(|i| self.last_test_ratios[i as usize])
                .unwrap_or(0.0);

            let summary = if self.switched_to_degraded {
                format!(
                    "EKF2 selector whipsaw: {} switches in {:.1}s, \
                     selector switched to a degraded instance \
                     (combined_test_ratio >= {:.1})",
                    switches_in_window,
                    window_duration_us as f64 / 1_000_000.0,
                    DEGRADED_TEST_RATIO,
                )
            } else {
                format!(
                    "EKF2 selector whipsaw: {} switches in {:.1}s \
                     (avg interval {:.0}ms)",
                    switches_in_window,
                    window_duration_us as f64 / 1_000_000.0,
                    avg_interval_ms,
                )
            };

            self.detections.push(Diagnostic {
                id: "ekf_selector_whipsaw".to_string(),
                summary,
                severity,
                timestamp_us: window_start,
                end_timestamp_us: Some(ts),
                evidence: Evidence::EkfSelectorWhipsaw {
                    switch_count: switches_in_window,
                    window_duration_ms: window_duration_us / 1_000,
                    avg_switch_interval_ms: avg_interval_ms,
                    switched_to_degraded: self.switched_to_degraded,
                    primary_instance_test_ratio: primary_test_ratio,
                },
            });

            // Reset the degraded flag for the next window so each detection
            // window is evaluated independently.
            self.switched_to_degraded = false;
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

    fn selector_msg(
        ts: u64,
        primary: u8,
        changed_count: u32,
        ratios: &[(usize, f32)],
    ) -> (px4_ulog::stream_parser::model::FlattenedFormat, Vec<u8>) {
        let mut b = MessageBuilder::new("estimator_selector_status")
            .timestamp(ts)
            .field_u8("primary_instance", primary)
            .field_u8("instances_available", 2)
            .field_u32("instance_changed_count", changed_count);
        for (i, r) in ratios {
            b = b.field_f32(&format!("combined_test_ratio[{i}]"), *r);
        }
        b.build()
    }

    #[test]
    fn no_false_positives_sample() {
        assert_no_false_positives("sample.ulg", "ekf_selector_whipsaw");
    }

    #[test]
    fn detects_rapid_switching() {
        let mut analyzer = EkfSelectorWhipsawAnalyzer::new();

        // 5 switches within 5 seconds
        for i in 0..5 {
            let ts = (i as u64 + 1) * 1_000_000;
            let primary = if i % 2 == 0 { 0 } else { 1 };
            let (fmt, data) = selector_msg(
                ts,
                primary,
                i as u32,
                &[(0, 0.5), (1, 0.5)],
            );
            let dm = make_data_message(&fmt, &data);
            analyzer.on_message(&dm);
        }

        let diags = Box::new(analyzer).finish();
        assert!(!diags.is_empty(), "Should detect rapid switching");
    }

    #[test]
    fn flags_critical_when_switching_to_degraded() {
        let mut analyzer = EkfSelectorWhipsawAnalyzer::new();

        // Instance 0 has combined_test_ratio >= 1.0 (degraded).
        // Selector switches to it. Should be critical.
        for i in 0..5 {
            let ts = (i as u64 + 1) * 1_000_000;
            let primary = if i % 2 == 0 { 0 } else { 1 };
            let (fmt, data) = selector_msg(
                ts,
                primary,
                i as u32,
                &[(0, 2.0), (1, 0.3)],
            );
            let dm = make_data_message(&fmt, &data);
            analyzer.on_message(&dm);
        }

        let diags = Box::new(analyzer).finish();
        assert!(!diags.is_empty());
        assert_eq!(diags[0].severity, Severity::Critical);
    }

    #[test]
    fn ignores_single_ekf() {
        let mut analyzer = EkfSelectorWhipsawAnalyzer::new();

        // Only 1 instance available — multi-EKF not active, should be ignored
        for i in 0..5 {
            let ts = (i as u64 + 1) * 1_000_000;
            let (fmt, data) = MessageBuilder::new("estimator_selector_status")
                .timestamp(ts)
                .field_u8("primary_instance", 0)
                .field_u8("instances_available", 1)
                .field_u32("instance_changed_count", i as u32)
                .build();
            let dm = make_data_message(&fmt, &data);
            analyzer.on_message(&dm);
        }

        let diags = Box::new(analyzer).finish();
        assert!(diags.is_empty(), "Single-EKF should not fire");
    }

    #[test]
    fn ignores_stable_selector() {
        let mut analyzer = EkfSelectorWhipsawAnalyzer::new();

        for i in 0..10 {
            let ts = (i as u64 + 1) * 1_000_000;
            let (fmt, data) = selector_msg(
                ts,
                0,
                0, // no switches
                &[(0, 0.3), (1, 0.4)],
            );
            let dm = make_data_message(&fmt, &data);
            analyzer.on_message(&dm);
        }

        let diags = Box::new(analyzer).finish();
        assert!(diags.is_empty(), "Stable selector should not fire");
    }

    #[test]
    fn handles_missing_fields() {
        let mut analyzer = EkfSelectorWhipsawAnalyzer::new();

        let (fmt, data) = MessageBuilder::new("estimator_selector_status")
            .timestamp(1_000_000)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm); // must not panic

        let diags = Box::new(analyzer).finish();
        assert!(diags.is_empty());
    }

    #[test]
    fn snapshot_sample_ulg() {
        let diags = analyze_fixture_for("sample.ulg", "ekf_selector_whipsaw");
        insta::assert_json_snapshot!(diags);
    }

    #[test]
    fn detects_real_ekf_selector_whipsaw() {
        let diags = analyze_fixture_for(
            "ekf_selector_whipsaw.ulg",
            "ekf_selector_whipsaw",
        );
        assert!(
            !diags.is_empty(),
            "Should detect whipsaw in real PX4 #27013 log"
        );
        assert!(
            diags.iter().any(|d| d.severity == Severity::Critical),
            "Real-world #27013 log should have at least one critical detection"
        );
        assert!(
            diags.iter().any(|d| matches!(
                &d.evidence,
                Evidence::EkfSelectorWhipsaw {
                    switched_to_degraded: true,
                    ..
                }
            )),
            "Real-world #27013 log should show switched_to_degraded"
        );
        insta::assert_json_snapshot!(diags);
    }
}
