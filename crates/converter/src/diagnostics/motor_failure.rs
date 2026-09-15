//! Actuator command discontinuities while armed (legacy ID: motor_failure).
//! Outputs are commands in driver-specific units, not motor feedback. Without
//! output-function mappings and driver limits we cannot identify motors, infer
//! saturation from 1900, or diagnose a locked/disconnected rotor.
//!
//! NEGATIVE_FIXTURE: motor_failure.ulg has separate output banks, not the
//! formerly asserted six simultaneous drops. A labeled positive fixture is needed.

use super::{
    parse_field, Analyzer, AnomalyKind, Diagnostic, Evidence, FieldUnit, MotorFailureMode,
    OutputDescriptor, PlotAnchor, Severity,
};
use crate::analysis::nav_state_name;
use px4_ulog::stream_parser::model::DataMessage;

/// Maximum number of motors to track.
const MAX_MOTORS: usize = 16;

pub struct MotorFailureAnalyzer {
    armed: bool,
    current_flight_mode: String,
    previous_outputs: [Option<f32>; MAX_MOTORS],
    last_sample_us: Option<u64>,
    detections: Vec<Diagnostic>,
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
            previous_outputs: [None; MAX_MOTORS],
            last_sample_us: None,
            detections: Vec::new(),
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
        "Actuator command dropped to zero while armed (not motor feedback)"
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
                    if !self.armed {
                        self.previous_outputs.fill(None);
                    }
                }
                if let Some(nav) = parse_field::<u8>(data, "nav_state") {
                    self.current_flight_mode = nav_state_name(nav).to_string();
                }
            }
            "actuator_outputs" => {
                if !self.armed {
                    return;
                }

                let Some(ts) = super::timestamp(data) else {
                    return;
                };
                if self.last_sample_us.is_some_and(|previous| ts <= previous) {
                    return;
                }
                self.last_sample_us = Some(ts);
                let count = parse_field::<u32>(data, "noutputs")
                    .map(|count| count.min(MAX_MOTORS as u32) as usize)
                    .unwrap_or_else(|| Self::detect_motor_count(data));
                self.previous_outputs[count..].fill(None);
                for i in 0..count {
                    let field_name = format!("output[{i}]");
                    let Some(pwm) =
                        parse_field::<f32>(data, &field_name).filter(|value| value.is_finite())
                    else {
                        self.previous_outputs[i] = None;
                        continue;
                    };
                    let motor_idx = i as u8;
                    let dropped = pwm == 0.0 && self.previous_outputs[i].is_some_and(|v| v > 0.0);
                    self.previous_outputs[i] = Some(pwm);
                    if dropped {
                        self.detections.push(Diagnostic {
                            id: "motor_failure".to_string(),
                            summary: format!(
                                "Actuator output {} command dropped to zero at {:.1}s while armed in {} mode",
                                i,
                                ts as f64 / 1_000_000.0,
                                self.current_flight_mode
                            ),
                            severity: Severity::Warning,
                            kind: AnomalyKind::Point,
                            timestamp_us: ts,
                            anchor: PlotAnchor::new("actuator_outputs", &format!("output[{i}]")),
                            descriptor: self.output_descriptor(),
                            evidence: Evidence::MotorFailure {
                                motor_index: motor_idx,
                                pwm_value: pwm,
                                mode: MotorFailureMode::DropToZero,
                                flight_mode: self.current_flight_mode.clone(),
                            },
                        });
                    }
                }
            }
            _ => {}
        }
    }

    fn finish(self: Box<Self>) -> Vec<Diagnostic> {
        self.detections
    }

    fn output_descriptor(&self) -> OutputDescriptor {
        OutputDescriptor::new()
            .field("motor_index", FieldUnit::Count)
            .field("pwm_value", FieldUnit::ActuatorOutput)
            .field("flight_mode", FieldUnit::Label)
            .field("mode", FieldUnit::Label)
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
        assert_eq!(diags[0].severity, Severity::Warning);
        assert_eq!(diags[0].kind, AnomalyKind::Point);
        assert_eq!(diags[0].anchor.topic, "actuator_outputs");
        assert_eq!(diags[0].anchor.field, "output[1]");
        match &diags[0].evidence {
            Evidence::MotorFailure {
                motor_index, mode, ..
            } => {
                assert_eq!(*motor_index, 1);
                assert_eq!(*mode, MotorFailureMode::DropToZero);
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
        assert!(
            diags.is_empty(),
            "Unused channels should not trigger motor_failure, got: {:?}",
            diags
        );
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
        assert_eq!(
            diags.len(),
            1,
            "Should only fire once per motor per failure mode"
        );
    }

    #[test]
    fn snapshot_sample_ulg() {
        let diags = analyze_fixture_for("sample.ulg", "motor_failure");
        insta::assert_json_snapshot!(diags);
    }

    // ---- Real-world fixture test ----
    #[test]
    fn no_false_positives_multi_output_fixture() {
        let diags = analyze_fixture_for("motor_failure.ulg", "motor_failure");
        // Instance 0 is always zero; instances 1/2 are separate active outputs.
        // The only actual instance-1 zero transitions occur after disarm.
        assert!(diags.is_empty());
        insta::assert_json_snapshot!(diags);
    }
}
