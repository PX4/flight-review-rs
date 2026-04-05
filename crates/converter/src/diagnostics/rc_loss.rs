//! RC signal loss detection analyzer.
//!
//! Detects loss of RC (remote control) signal during armed flight.
//! Tracks the `rc_lost` field from `input_rc` and the armed state from
//! `vehicle_status`.
//!
//! SKIP_FIXTURE: No known ULog in our corpus exhibits RC signal loss.
//! Add a fixture when one becomes available.

use super::{parse_field, Analyzer, Diagnostic, Evidence, Severity};
use px4_ulog::stream_parser::model::DataMessage;

/// Minimum loss duration (microseconds) to report.
const MIN_LOSS_DURATION_US: u64 = 500_000;
/// Loss duration threshold for critical severity (5 seconds).
const CRITICAL_DURATION_US: u64 = 5_000_000;

pub struct RcLossAnalyzer {
    armed: bool,
    rc_lost: bool,
    loss_start_us: Option<u64>,
    last_signal_us: u64,
    detections: Vec<Diagnostic>,
}

impl Default for RcLossAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl RcLossAnalyzer {
    pub fn new() -> Self {
        Self {
            armed: false,
            rc_lost: false,
            loss_start_us: None,
            last_signal_us: 0,
            detections: Vec::new(),
        }
    }

    fn emit_loss(&mut self, start_us: u64, end_us: u64) {
        let duration_us = end_us.saturating_sub(start_us);
        if duration_us < MIN_LOSS_DURATION_US {
            return;
        }

        let severity = if duration_us >= CRITICAL_DURATION_US {
            Severity::Critical
        } else {
            Severity::Warning
        };

        let duration_ms = duration_us / 1_000;
        self.detections.push(Diagnostic {
            id: "rc_loss".to_string(),
            summary: format!(
                "RC signal lost for {:.1}s starting at {:.1}s while armed",
                duration_us as f64 / 1_000_000.0,
                start_us as f64 / 1_000_000.0,
            ),
            severity,
            timestamp_us: start_us,
            end_timestamp_us: Some(end_us),
            evidence: Evidence::RcLoss {
                last_signal_timestamp_us: self.last_signal_us,
                signal_lost_duration_ms: duration_ms,
            },
        });
    }
}

impl Analyzer for RcLossAnalyzer {
    fn id(&self) -> &str {
        "rc_loss"
    }

    fn description(&self) -> &str {
        "RC signal loss during armed flight"
    }

    fn required_topics(&self) -> &[&str] {
        &["input_rc", "vehicle_status"]
    }

    fn on_message(&mut self, data: &DataMessage) {
        let topic = data.flattened_format.message_name.as_str();
        let ts = data
            .flattened_format
            .timestamp_field
            .as_ref()
            .map(|tf| tf.parse_timestamp(data.data))
            .unwrap_or(0);

        match topic {
            "vehicle_status" => {
                let was_armed = self.armed;
                if let Some(arming) = parse_field::<u8>(data, "arming_state") {
                    self.armed = arming == 2;
                }
                // If we just disarmed while RC was lost, close the loss window
                if was_armed && !self.armed {
                    if let Some(start) = self.loss_start_us.take() {
                        self.emit_loss(start, ts);
                    }
                    self.rc_lost = false;
                }
            }
            "input_rc" => {
                let lost = parse_field::<u8>(data, "rc_lost")
                    .map(|v| v != 0)
                    .unwrap_or(false);

                if lost && self.armed && !self.rc_lost {
                    // RC just lost while armed
                    self.rc_lost = true;
                    self.loss_start_us = Some(ts);
                } else if !lost && self.rc_lost {
                    // RC recovered
                    self.rc_lost = false;
                    if let Some(start) = self.loss_start_us.take() {
                        if self.armed {
                            self.emit_loss(start, ts);
                        }
                    }
                }

                if !lost {
                    self.last_signal_us = ts;
                }
            }
            _ => {}
        }
    }

    fn finish(mut self: Box<Self>) -> Vec<Diagnostic> {
        // If RC was still lost at end of log, close the window
        if self.rc_lost && self.armed {
            if let Some(start) = self.loss_start_us.take() {
                // Use last known timestamp as end
                let end = self.last_signal_us.max(start);
                self.emit_loss(start, end);
            }
        }
        self.detections
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::testing::*;

    #[test]
    fn no_false_positives_sample() {
        assert_no_false_positives("sample.ulg", "rc_loss");
    }

    #[test]
    fn detects_rc_loss_during_armed_flight() {
        let mut analyzer = RcLossAnalyzer::new();

        // Arm
        let (fmt, data) = MessageBuilder::new("vehicle_status")
            .timestamp(1_000_000)
            .field_u8("arming_state", 2)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm);

        // RC signal present
        let (fmt2, data2) = MessageBuilder::new("input_rc")
            .timestamp(2_000_000)
            .field_u8("rc_lost", 0)
            .build();
        let dm2 = make_data_message(&fmt2, &data2);
        analyzer.on_message(&dm2);

        // RC lost
        let (fmt3, data3) = MessageBuilder::new("input_rc")
            .timestamp(3_000_000)
            .field_u8("rc_lost", 1)
            .build();
        let dm3 = make_data_message(&fmt3, &data3);
        analyzer.on_message(&dm3);

        // RC recovered after 2 seconds (> min threshold)
        let (fmt4, data4) = MessageBuilder::new("input_rc")
            .timestamp(5_000_000)
            .field_u8("rc_lost", 0)
            .build();
        let dm4 = make_data_message(&fmt4, &data4);
        analyzer.on_message(&dm4);

        let diags = Box::new(analyzer).finish();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Warning);
        match &diags[0].evidence {
            Evidence::RcLoss {
                signal_lost_duration_ms,
                ..
            } => {
                assert_eq!(*signal_lost_duration_ms, 2000);
            }
            _ => panic!("Expected RcLoss evidence"),
        }
    }

    #[test]
    fn critical_on_long_loss() {
        let mut analyzer = RcLossAnalyzer::new();

        let (fmt, data) = MessageBuilder::new("vehicle_status")
            .timestamp(1_000_000)
            .field_u8("arming_state", 2)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm);

        // RC lost
        let (fmt2, data2) = MessageBuilder::new("input_rc")
            .timestamp(2_000_000)
            .field_u8("rc_lost", 1)
            .build();
        let dm2 = make_data_message(&fmt2, &data2);
        analyzer.on_message(&dm2);

        // RC recovered after 6 seconds
        let (fmt3, data3) = MessageBuilder::new("input_rc")
            .timestamp(8_000_000)
            .field_u8("rc_lost", 0)
            .build();
        let dm3 = make_data_message(&fmt3, &data3);
        analyzer.on_message(&dm3);

        let diags = Box::new(analyzer).finish();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Critical);
    }

    #[test]
    fn no_detection_when_disarmed() {
        let mut analyzer = RcLossAnalyzer::new();

        // Disarmed
        let (fmt, data) = MessageBuilder::new("vehicle_status")
            .timestamp(1_000_000)
            .field_u8("arming_state", 0)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm);

        // RC lost while disarmed
        let (fmt2, data2) = MessageBuilder::new("input_rc")
            .timestamp(2_000_000)
            .field_u8("rc_lost", 1)
            .build();
        let dm2 = make_data_message(&fmt2, &data2);
        analyzer.on_message(&dm2);

        let (fmt3, data3) = MessageBuilder::new("input_rc")
            .timestamp(10_000_000)
            .field_u8("rc_lost", 0)
            .build();
        let dm3 = make_data_message(&fmt3, &data3);
        analyzer.on_message(&dm3);

        let diags = Box::new(analyzer).finish();
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_short_loss() {
        let mut analyzer = RcLossAnalyzer::new();

        let (fmt, data) = MessageBuilder::new("vehicle_status")
            .timestamp(1_000_000)
            .field_u8("arming_state", 2)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm);

        // Very short RC loss (200ms, below 500ms threshold)
        let (fmt2, data2) = MessageBuilder::new("input_rc")
            .timestamp(2_000_000)
            .field_u8("rc_lost", 1)
            .build();
        let dm2 = make_data_message(&fmt2, &data2);
        analyzer.on_message(&dm2);

        let (fmt3, data3) = MessageBuilder::new("input_rc")
            .timestamp(2_200_000)
            .field_u8("rc_lost", 0)
            .build();
        let dm3 = make_data_message(&fmt3, &data3);
        analyzer.on_message(&dm3);

        let diags = Box::new(analyzer).finish();
        assert!(diags.is_empty());
    }

    #[test]
    fn handles_missing_fields() {
        let mut analyzer = RcLossAnalyzer::new();

        let (fmt, data) = MessageBuilder::new("input_rc")
            .timestamp(1_000_000)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm); // must not panic

        let diags = Box::new(analyzer).finish();
        assert!(diags.is_empty());
    }

    #[test]
    fn snapshot_sample_ulg() {
        let diags = analyze_fixture_for("sample.ulg", "rc_loss");
        insta::assert_json_snapshot!(diags);
    }
}
