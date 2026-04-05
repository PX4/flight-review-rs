//! Motor failure detection analyzer.
//!
//! Detects two failure modes while the vehicle is armed:
//! - **PWM drop to zero**: A motor output suddenly drops to 0, indicating
//!   a motor disconnect or ESC failure.
//! - **Locked at max**: A motor output saturates at maximum PWM for a
//!   sustained period, indicating a locked rotor or mechanical failure.

use std::collections::{HashSet, VecDeque};

use super::{parse_field, Analyzer, Diagnostic, Evidence, Severity};
use crate::analysis::nav_state_name;
use px4_ulog::stream_parser::model::DataMessage;

/// Maximum number of motors to track.
const MAX_MOTORS: usize = 16;
/// Sliding window size per motor.
const WINDOW_SIZE: usize = 50;
/// PWM threshold for "locked at max" detection.
const PWM_MAX_THRESHOLD: f32 = 1900.0;

pub struct MotorFailureAnalyzer {
    armed: bool,
    current_flight_mode: String,
    motor_windows: Vec<VecDeque<(u64, f32)>>,
    detections: Vec<Diagnostic>,
    /// Track which (motor_index, failure_mode) pairs have already fired.
    fired: HashSet<(u8, String)>,
    /// Track which motors have been active (non-zero output while armed).
    /// Unused channels stay at 0 and should not be flagged.
    motor_was_active: Vec<bool>,
    /// Number of motors detected from the log.
    motor_count: Option<usize>,
}

impl Default for MotorFailureAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl MotorFailureAnalyzer {
    pub fn new() -> Self {
        Self {
            armed: false,
            current_flight_mode: "Unknown".to_string(),
            motor_windows: Vec::new(),
            detections: Vec::new(),
            fired: HashSet::new(),
            motor_was_active: Vec::new(),
            motor_count: None,
        }
    }

    fn detect_motor_count(data: &DataMessage) -> usize {
        let mut count = 0;
        for i in 0..MAX_MOTORS {
            let field_name = format!("output[{i}]");
            if parse_field::<f32>(data, &field_name).is_some() {
                count = i + 1;
            } else {
                break;
            }
        }
        count
    }
}

impl Analyzer for MotorFailureAnalyzer {
    fn id(&self) -> &str {
        "motor_failure"
    }

    fn description(&self) -> &str {
        "PWM drop/lock detection while armed"
    }

    fn required_topics(&self) -> &[&str] {
        &["actuator_outputs", "vehicle_status"]
    }

    fn on_message(&mut self, data: &DataMessage) {
        let topic = data.flattened_format.message_name.as_str();

        match topic {
            "vehicle_status" => {
                if let Some(arming) = parse_field::<u8>(data, "arming_state") {
                    self.armed = arming == 2;
                }
                if let Some(nav) = parse_field::<u8>(data, "nav_state") {
                    self.current_flight_mode = nav_state_name(nav).to_string();
                }
            }
            "actuator_outputs" => {
                if !self.armed {
                    return;
                }

                let ts = data
                    .flattened_format
                    .timestamp_field
                    .as_ref()
                    .map(|tf| tf.parse_timestamp(data.data))
                    .unwrap_or(0);

                // Auto-detect motor count on first message
                if self.motor_count.is_none() {
                    let count = Self::detect_motor_count(data);
                    self.motor_count = Some(count);
                    self.motor_windows = (0..count).map(|_| VecDeque::with_capacity(WINDOW_SIZE)).collect();
                    self.motor_was_active = vec![false; count];
                }

                let count = self.motor_count.unwrap_or(0);
                for i in 0..count {
                    let field_name = format!("output[{i}]");
                    let Some(pwm) = parse_field::<f32>(data, &field_name) else {
                        continue;
                    };

                    if let Some(window) = self.motor_windows.get_mut(i) {
                        window.push_back((ts, pwm));
                        if window.len() > WINDOW_SIZE {
                            window.pop_front();
                        }
                    }

                    let motor_idx = i as u8;

                    // Track which motors have been active (non-zero while armed)
                    if pwm != 0.0 {
                        if let Some(active) = self.motor_was_active.get_mut(i) {
                            *active = true;
                        }
                    }

                    // Check: PWM drop to zero — only if motor was previously active
                    let was_active = self.motor_was_active.get(i).copied().unwrap_or(false);
                    if pwm == 0.0 && was_active && !self.fired.contains(&(motor_idx, "drop_to_zero".to_string())) {
                        self.fired.insert((motor_idx, "drop_to_zero".to_string()));
                        self.detections.push(Diagnostic {
                            id: "motor_failure".to_string(),
                            summary: format!(
                                "Motor {} output dropped to 0 PWM at {:.1}s while armed in {} mode",
                                i,
                                ts as f64 / 1_000_000.0,
                                self.current_flight_mode
                            ),
                            severity: Severity::Critical,
                            timestamp_us: ts,
                            end_timestamp_us: None,
                            evidence: Evidence::MotorFailure {
                                motor_index: motor_idx,
                                pwm_value: pwm,
                                failure_mode: "drop_to_zero".to_string(),
                                flight_mode: self.current_flight_mode.clone(),
                            },
                        });
                    }

                    // Check: Locked at max
                    if let Some(window) = self.motor_windows.get(i) {
                        if window.len() == WINDOW_SIZE
                            && window.iter().all(|(_, p)| *p >= PWM_MAX_THRESHOLD)
                            && !self.fired.contains(&(motor_idx, "locked_at_max".to_string()))
                        {
                            self.fired.insert((motor_idx, "locked_at_max".to_string()));
                            let first_ts = window.front().map(|(t, _)| *t).unwrap_or(ts);
                            self.detections.push(Diagnostic {
                                id: "motor_failure".to_string(),
                                summary: format!(
                                    "Motor {} locked at max PWM from {:.1}s to {:.1}s while armed",
                                    i,
                                    first_ts as f64 / 1_000_000.0,
                                    ts as f64 / 1_000_000.0,
                                ),
                                severity: Severity::Warning,
                                timestamp_us: first_ts,
                                end_timestamp_us: Some(ts),
                                evidence: Evidence::MotorFailure {
                                    motor_index: motor_idx,
                                    pwm_value: pwm,
                                    failure_mode: "locked_at_max".to_string(),
                                    flight_mode: self.current_flight_mode.clone(),
                                },
                            });
                        }
                    }
                }
            }
            _ => {}
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
        assert_no_false_positives("sample.ulg", "motor_failure");
    }

    #[test]
    fn detects_pwm_drop_to_zero() {
        let mut analyzer = MotorFailureAnalyzer::new();

        // Arm the vehicle
        let (fmt, data) = MessageBuilder::new("vehicle_status")
            .timestamp(1_000_000)
            .field_u8("arming_state", 2)
            .field_u8("nav_state", 2)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm);

        // Both motors active initially
        let (fmt1, data1) = MessageBuilder::new("actuator_outputs")
            .timestamp(1_500_000)
            .field_f32("output[0]", 1500.0)
            .field_f32("output[1]", 1500.0)
            .build();
        let dm1 = make_data_message(&fmt1, &data1);
        analyzer.on_message(&dm1);

        // Motor 1 drops to zero
        let (fmt2, data2) = MessageBuilder::new("actuator_outputs")
            .timestamp(2_000_000)
            .field_f32("output[0]", 1500.0)
            .field_f32("output[1]", 0.0)
            .build();
        let dm2 = make_data_message(&fmt2, &data2);
        analyzer.on_message(&dm2);

        let diags = Box::new(analyzer).finish();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Critical);
        match &diags[0].evidence {
            Evidence::MotorFailure {
                motor_index,
                failure_mode,
                ..
            } => {
                assert_eq!(*motor_index, 1);
                assert_eq!(failure_mode, "drop_to_zero");
            }
            _ => panic!("Expected MotorFailure evidence"),
        }
    }

    #[test]
    fn ignores_unused_motor_channels() {
        let mut analyzer = MotorFailureAnalyzer::new();

        // Arm
        let (fmt, data) = MessageBuilder::new("vehicle_status")
            .timestamp(1_000_000)
            .field_u8("arming_state", 2)
            .field_u8("nav_state", 2)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm);

        // 4 active motors, outputs 4-7 always zero (unused channels)
        for ts in [2_000_000u64, 3_000_000, 4_000_000] {
            let (fmt2, data2) = MessageBuilder::new("actuator_outputs")
                .timestamp(ts)
                .field_f32("output[0]", 1500.0)
                .field_f32("output[1]", 1500.0)
                .field_f32("output[2]", 1500.0)
                .field_f32("output[3]", 1500.0)
                .field_f32("output[4]", 0.0)
                .field_f32("output[5]", 0.0)
                .field_f32("output[6]", 0.0)
                .field_f32("output[7]", 0.0)
                .build();
            let dm2 = make_data_message(&fmt2, &data2);
            analyzer.on_message(&dm2);
        }

        let diags = Box::new(analyzer).finish();
        assert!(diags.is_empty(), "Unused channels should not trigger motor_failure, got: {:?}", diags);
    }

    #[test]
    fn no_detection_when_disarmed() {
        let mut analyzer = MotorFailureAnalyzer::new();

        // Disarmed (arming_state != 2)
        let (fmt, data) = MessageBuilder::new("vehicle_status")
            .timestamp(1_000_000)
            .field_u8("arming_state", 0)
            .field_u8("nav_state", 0)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm);

        // Motor at zero while disarmed — should not trigger
        let (fmt2, data2) = MessageBuilder::new("actuator_outputs")
            .timestamp(2_000_000)
            .field_f32("output[0]", 0.0)
            .build();
        let dm2 = make_data_message(&fmt2, &data2);
        analyzer.on_message(&dm2);

        let diags = Box::new(analyzer).finish();
        assert!(diags.is_empty());
    }

    #[test]
    fn handles_missing_fields() {
        let mut analyzer = MotorFailureAnalyzer::new();

        // Arm first
        let (fmt, data) = MessageBuilder::new("vehicle_status")
            .timestamp(1_000_000)
            .field_u8("arming_state", 2)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm);

        // actuator_outputs with no output fields
        let (fmt2, data2) = MessageBuilder::new("actuator_outputs")
            .timestamp(2_000_000)
            .build();
        let dm2 = make_data_message(&fmt2, &data2);
        analyzer.on_message(&dm2); // must not panic

        let diags = Box::new(analyzer).finish();
        assert!(diags.is_empty());
    }

    #[test]
    fn deduplicates_repeated_failures() {
        let mut analyzer = MotorFailureAnalyzer::new();

        let (fmt, data) = MessageBuilder::new("vehicle_status")
            .timestamp(1_000_000)
            .field_u8("arming_state", 2)
            .field_u8("nav_state", 2)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm);

        // Motor active first
        let (fmt1, data1) = MessageBuilder::new("actuator_outputs")
            .timestamp(1_500_000)
            .field_f32("output[0]", 1500.0)
            .build();
        let dm1 = make_data_message(&fmt1, &data1);
        analyzer.on_message(&dm1);

        // Same motor drops to zero multiple times
        for ts in [2_000_000u64, 3_000_000, 4_000_000] {
            let (fmt2, data2) = MessageBuilder::new("actuator_outputs")
                .timestamp(ts)
                .field_f32("output[0]", 0.0)
                .build();
            let dm2 = make_data_message(&fmt2, &data2);
            analyzer.on_message(&dm2);
        }

        let diags = Box::new(analyzer).finish();
        assert_eq!(diags.len(), 1, "Should only fire once per motor per failure mode");
    }

    #[test]
    fn snapshot_sample_ulg() {
        let diags = analyze_fixture_for("sample.ulg", "motor_failure");
        insta::assert_json_snapshot!(diags);
    }

    // ---- Real-world fixture test ----
    #[test]
    fn detects_real_motor_failure() {
        let diags = analyze_fixture_for("motor_failure.ulg", "motor_failure");
        assert!(
            !diags.is_empty(),
            "Should detect motor failures in real crash log"
        );
        assert!(
            diags.iter().any(|d| d.severity == Severity::Critical),
            "Should have at least one critical motor failure"
        );
        insta::assert_json_snapshot!(diags);
    }
}
