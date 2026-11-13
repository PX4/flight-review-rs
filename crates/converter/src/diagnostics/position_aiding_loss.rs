//! In-air loss of the horizontal position estimate.
//!
//! Tracks `vehicle_local_position.xy_valid` (the selected estimator output)
//! and reports a region when it goes from valid to invalid while armed and
//! `vehicle_land_detected.landed` is false. In EKF2 the flag is
//! `isLocalHorizontalPositionValid()`: it only becomes valid once horizontal
//! aiding has been counted and turns invalid after `EKF2_NOAID_TOUT` without
//! it, so a vehicle that never had aiding never reports valid and never
//! fires.
//!
//! Regions end when `xy_valid` is valid again, on landing, on disarm, or at
//! the last observed invalid sample (end of log or unavailable flag); the end
//! is never extrapolated. Unknown `xy_valid` or `landed` values are neither
//! valid nor invalid. `vehicle_local_position` and `vehicle_land_detected`
//! share instance 0, so `PerInstance` drops a land-detected sample older than
//! the last local position sample. A file-order scan of the repo fixtures
//! found no in-flight land-detected sample dropped this way.
//!
//! Fixture `position_aiding_loss.ulg` is log 0dce354d (PX4 v1.18.0-rc1,
//! FMU-v6C, flight-review-rs #53): EKF2 never passed its GNSS checks, takeoff
//! detected at 153.792s, `xy_valid` lost at 154.258s, landed at 159.901s.

use super::{
    parse_bool, parse_field, Analyzer, AnomalyKind, Diagnostic, Evidence, FieldUnit,
    OutputDescriptor, PlotAnchor, PositionAidingLossEnd, Severity,
};
use px4_ulog::stream_parser::model::DataMessage;

const ID: &str = "position_aiding_loss";
const TOPIC: &str = "vehicle_local_position";
const FIELD: &str = "xy_valid";
/// `vehicle_status.arming_state` value for armed.
const ARMING_STATE_ARMED: u8 = 2;

struct Loss {
    start_us: u64,
    last_valid_us: u64,
    last_invalid_us: u64,
}

pub struct PositionAidingLossAnalyzer {
    armed: bool,
    landed: Option<bool>,
    /// Last `xy_valid` sample: (timestamp, value). None after an unknown value.
    last_xy: Option<(u64, bool)>,
    loss: Option<Loss>,
    detections: Vec<Diagnostic>,
}

impl Default for PositionAidingLossAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl PositionAidingLossAnalyzer {
    pub fn new() -> Self {
        Self {
            armed: false,
            landed: None,
            last_xy: None,
            loss: None,
            detections: Vec::new(),
        }
    }

    fn close(&mut self, end_us: u64, ended_by: PositionAidingLossEnd) {
        let Some(loss) = self.loss.take() else {
            return;
        };
        let end_us = end_us.max(loss.start_us);
        let duration_us = end_us - loss.start_us;
        let reason = match ended_by {
            PositionAidingLossEnd::Recovered => "until it was valid again",
            PositionAidingLossEnd::Landed => "until landing",
            PositionAidingLossEnd::Disarmed => "until disarm",
            PositionAidingLossEnd::Unobserved => "until the last observed sample",
        };
        self.detections.push(Diagnostic {
            id: ID.to_string(),
            summary: format!(
                "Horizontal position estimate became invalid in air at {:.1}s, {:.1}s {reason}",
                loss.start_us as f64 / 1_000_000.0,
                duration_us as f64 / 1_000_000.0,
            ),
            severity: Severity::Warning,
            kind: AnomalyKind::Region {
                end_timestamp_us: end_us,
            },
            timestamp_us: loss.start_us,
            anchor: PlotAnchor::new(TOPIC, FIELD),
            descriptor: self.output_descriptor(),
            evidence: Evidence::PositionAidingLoss {
                last_valid_timestamp_us: loss.last_valid_us,
                invalid_duration_ms: duration_us / 1_000,
                ended_by,
            },
        });
    }
}

impl Analyzer for PositionAidingLossAnalyzer {
    fn id(&self) -> &str {
        ID
    }

    fn description(&self) -> &str {
        "Horizontal position estimate lost while armed in air"
    }

    fn required_topics(&self) -> &[&str] {
        &[TOPIC, "vehicle_land_detected", "vehicle_status"]
    }

    fn on_message(&mut self, data: &DataMessage) {
        let ts = data
            .flattened_format
            .timestamp_field
            .as_ref()
            .map(|tf| tf.parse_timestamp(data.data))
            .unwrap_or(0);

        match data.flattened_format.message_name.as_str() {
            "vehicle_status" => {
                if let Some(arming) = parse_field::<u8>(data, "arming_state") {
                    self.armed = arming == ARMING_STATE_ARMED;
                }
                if !self.armed {
                    self.close(ts, PositionAidingLossEnd::Disarmed);
                }
            }
            "vehicle_land_detected" => {
                self.landed = parse_bool(data, "landed");
                if self.landed == Some(true) {
                    self.close(ts, PositionAidingLossEnd::Landed);
                }
            }
            TOPIC => {
                let Some(valid) = parse_bool(data, FIELD) else {
                    if let Some(last) = self.loss.as_ref().map(|loss| loss.last_invalid_us) {
                        self.close(last, PositionAidingLossEnd::Unobserved);
                    }
                    self.last_xy = None;
                    return;
                };
                if valid {
                    self.close(ts, PositionAidingLossEnd::Recovered);
                } else if let Some(loss) = self.loss.as_mut() {
                    loss.last_invalid_us = ts;
                } else if let Some((last_valid_us, true)) = self.last_xy {
                    if self.armed && self.landed == Some(false) {
                        self.loss = Some(Loss {
                            start_us: ts,
                            last_valid_us,
                            last_invalid_us: ts,
                        });
                    }
                }
                self.last_xy = Some((ts, valid));
            }
            _ => {}
        }
    }

    fn finish(mut self: Box<Self>) -> Vec<Diagnostic> {
        if let Some(last) = self.loss.as_ref().map(|loss| loss.last_invalid_us) {
            self.close(last, PositionAidingLossEnd::Unobserved);
        }
        self.detections
    }

    fn output_descriptor(&self) -> OutputDescriptor {
        OutputDescriptor::new()
            .field("last_valid_timestamp_us", FieldUnit::Microseconds)
            .field("invalid_duration_ms", FieldUnit::Milliseconds)
            .field("ended_by", FieldUnit::Label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::testing::*;

    fn status(ts: u64, arming_state: u8) -> MessageBuilder {
        MessageBuilder::new("vehicle_status")
            .timestamp(ts)
            .field_u8("arming_state", arming_state)
    }

    fn land(ts: u64, landed: bool) -> MessageBuilder {
        MessageBuilder::new("vehicle_land_detected")
            .timestamp(ts)
            .field_bool("landed", landed)
    }

    fn position(ts: u64, xy_valid: bool) -> MessageBuilder {
        MessageBuilder::new(TOPIC)
            .timestamp(ts)
            .field_bool("xy_valid", xy_valid)
    }

    /// Armed and airborne at t=0..1s with a valid estimate at 1.5s.
    fn airborne() -> PositionAidingLossAnalyzer {
        let mut a = PositionAidingLossAnalyzer::new();
        feed(&mut a, status(0, ARMING_STATE_ARMED), 0);
        feed(&mut a, land(1_000_000, false), 0);
        feed(&mut a, position(1_500_000, true), 0);
        a
    }

    fn only(diags: &[Diagnostic]) -> (&Diagnostic, u64, u64, &PositionAidingLossEnd) {
        assert_eq!(diags.len(), 1, "{diags:?}");
        let d = &diags[0];
        let AnomalyKind::Region { end_timestamp_us } = d.kind else {
            panic!("expected a region: {d:?}");
        };
        let Evidence::PositionAidingLoss {
            last_valid_timestamp_us,
            ref ended_by,
            ..
        } = d.evidence
        else {
            panic!("expected PositionAidingLoss evidence: {d:?}");
        };
        (d, end_timestamp_us, last_valid_timestamp_us, ended_by)
    }

    #[test]
    fn no_false_positives_sample() {
        assert_no_false_positives("sample.ulg", ID);
    }

    #[test]
    fn no_false_positives_fixtures_with_land_detector() {
        for fixture in [
            "battery_brownout.ulg",
            "ekf_failure.ulg",
            "ekf_selector_no_whipsaw.ulg",
            "fixed_wing_gps.ulg",
            "gps_interference.ulg",
            "motor_failure.ulg",
            "tecs_nonfinite_pitch.ulg",
        ] {
            assert_no_false_positives(fixture, ID);
        }
    }

    #[test]
    fn detects_real_position_aiding_loss() {
        let diags = analyze_fixture_for("position_aiding_loss.ulg", ID);
        let (d, end, last_valid, ended_by) = only(&diags);
        // Independently labeled with pyulog: xy_valid 1 -> 0 at 154.258s while
        // landed == 0 (takeoff 153.792s), landed again at 159.901s.
        assert!(
            (154_250_000..154_270_000).contains(&d.timestamp_us),
            "{d:?}"
        );
        assert!((159_890_000..159_910_000).contains(&end), "{d:?}");
        assert!(last_valid < d.timestamp_us && d.timestamp_us - last_valid < 100_000);
        assert_eq!(*ended_by, PositionAidingLossEnd::Landed);
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.anchor.topic, TOPIC);
        assert_eq!(d.anchor.field, FIELD);
        assert_eq!(d.anchor.instance, None);
    }

    #[test]
    fn detects_loss_and_recovery_in_air() {
        let mut a = airborne();
        feed(&mut a, position(2_000_000, false), 0);
        feed(&mut a, position(2_500_000, false), 0);
        feed(&mut a, position(3_000_000, true), 0);
        let diags = Box::new(a).finish();
        let (d, end, last_valid, ended_by) = only(&diags);
        assert_eq!(d.timestamp_us, 2_000_000);
        assert_eq!(end, 3_000_000);
        assert_eq!(last_valid, 1_500_000);
        assert_eq!(*ended_by, PositionAidingLossEnd::Recovered);
        let Evidence::PositionAidingLoss {
            invalid_duration_ms,
            ..
        } = d.evidence
        else {
            unreachable!()
        };
        assert_eq!(invalid_duration_ms, 1_000);
    }

    #[test]
    fn reports_each_loss_once() {
        let mut a = airborne();
        for i in 0..10 {
            feed(&mut a, position(2_000_000 + i * 100_000, false), 0);
        }
        feed(&mut a, position(4_000_000, true), 0);
        feed(&mut a, position(5_000_000, false), 0);
        feed(&mut a, position(6_000_000, true), 0);
        let diags = Box::new(a).finish();
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].timestamp_us, 2_000_000);
        assert_eq!(diags[1].timestamp_us, 5_000_000);
    }

    #[test]
    fn landing_and_disarm_end_the_region() {
        let mut a = airborne();
        feed(&mut a, position(2_000_000, false), 0);
        feed(&mut a, land(4_000_000, true), 0);
        feed(&mut a, position(4_500_000, false), 0);
        let diags = Box::new(a).finish();
        let (_, end, _, ended_by) = only(&diags);
        assert_eq!((end, ended_by), (4_000_000, &PositionAidingLossEnd::Landed));

        let mut a = airborne();
        feed(&mut a, position(2_000_000, false), 0);
        feed(&mut a, status(3_000_000, 1), 0);
        let diags = Box::new(a).finish();
        let (_, end, _, ended_by) = only(&diags);
        assert_eq!(
            (end, ended_by),
            (3_000_000, &PositionAidingLossEnd::Disarmed)
        );
    }

    #[test]
    fn end_of_log_uses_last_invalid_sample() {
        let mut a = airborne();
        feed(&mut a, position(2_000_000, false), 0);
        feed(&mut a, position(2_400_000, false), 0);
        let diags = Box::new(a).finish();
        let (_, end, _, ended_by) = only(&diags);
        assert_eq!(
            (end, ended_by),
            (2_400_000, &PositionAidingLossEnd::Unobserved)
        );
    }

    #[test]
    fn no_detection_on_ground_disarmed_or_without_prior_valid() {
        // Invalid on the ground, then takeoff without ever becoming valid.
        let mut a = PositionAidingLossAnalyzer::new();
        feed(&mut a, status(0, ARMING_STATE_ARMED), 0);
        feed(&mut a, land(500_000, true), 0);
        feed(&mut a, position(1_000_000, true), 0);
        feed(&mut a, position(2_000_000, false), 0);
        feed(&mut a, land(3_000_000, false), 0);
        feed(&mut a, position(4_000_000, false), 0);
        assert!(Box::new(a).finish().is_empty());

        // Never valid: no aiding at all.
        let mut a = PositionAidingLossAnalyzer::new();
        feed(&mut a, status(0, ARMING_STATE_ARMED), 0);
        feed(&mut a, land(500_000, false), 0);
        feed(&mut a, position(1_000_000, false), 0);
        feed(&mut a, position(2_000_000, false), 0);
        assert!(Box::new(a).finish().is_empty());

        // Disarmed (carried by hand, landed false).
        let mut a = PositionAidingLossAnalyzer::new();
        feed(&mut a, status(0, 1), 0);
        feed(&mut a, land(500_000, false), 0);
        feed(&mut a, position(1_000_000, true), 0);
        feed(&mut a, position(2_000_000, false), 0);
        assert!(Box::new(a).finish().is_empty());
    }

    #[test]
    fn unknown_values_break_tracking() {
        // Unknown landed state never counts as airborne.
        let mut a = PositionAidingLossAnalyzer::new();
        feed(&mut a, status(0, ARMING_STATE_ARMED), 0);
        feed(
            &mut a,
            MessageBuilder::new("vehicle_land_detected").timestamp(500_000),
            0,
        );
        feed(&mut a, position(1_000_000, true), 0);
        feed(&mut a, position(2_000_000, false), 0);
        assert!(Box::new(a).finish().is_empty());

        // An unknown xy_valid between valid and invalid is not an edge.
        let mut a = airborne();
        feed(&mut a, MessageBuilder::new(TOPIC).timestamp(2_000_000), 0);
        feed(&mut a, position(2_500_000, false), 0);
        assert!(Box::new(a).finish().is_empty());

        // An unknown xy_valid during a loss ends it at the last invalid sample.
        let mut a = airborne();
        feed(&mut a, position(2_000_000, false), 0);
        feed(&mut a, position(2_200_000, false), 0);
        feed(&mut a, MessageBuilder::new(TOPIC).timestamp(2_500_000), 0);
        feed(&mut a, position(3_000_000, true), 0);
        let diags = Box::new(a).finish();
        let (_, end, _, ended_by) = only(&diags);
        assert_eq!(
            (end, ended_by),
            (2_200_000, &PositionAidingLossEnd::Unobserved)
        );
    }

    #[test]
    fn legacy_uint8_flags_are_supported() {
        let mut a = PositionAidingLossAnalyzer::new();
        feed(&mut a, status(0, ARMING_STATE_ARMED), 0);
        feed(
            &mut a,
            MessageBuilder::new("vehicle_land_detected")
                .timestamp(500_000)
                .field_u8("landed", 0),
            0,
        );
        for (ts, value) in [
            (1_000_000, 1),
            (1_500_000, 2),
            (2_000_000, 1),
            (2_500_000, 0),
        ] {
            feed(
                &mut a,
                MessageBuilder::new(TOPIC)
                    .timestamp(ts)
                    .field_u8("xy_valid", value),
                0,
            );
        }
        let diags = Box::new(a).finish();
        let (d, _, last_valid, _) = only(&diags);
        assert_eq!((d.timestamp_us, last_valid), (2_500_000, 2_000_000));
    }

    #[test]
    fn handles_missing_fields() {
        let mut a = PositionAidingLossAnalyzer::new();
        for topic in [TOPIC, "vehicle_land_detected", "vehicle_status"] {
            feed(&mut a, MessageBuilder::new(topic).timestamp(1_000_000), 0);
        }
        assert!(Box::new(a).finish().is_empty());
    }

    #[test]
    fn snapshot_sample_ulg() {
        let diags = analyze_fixture_for("sample.ulg", ID);
        insta::assert_json_snapshot!(diags);
    }

    #[test]
    fn snapshot_detects_real_position_aiding_loss() {
        let diags = analyze_fixture_for("position_aiding_loss.ulg", ID);
        insta::assert_json_snapshot!(diags);
    }
}
