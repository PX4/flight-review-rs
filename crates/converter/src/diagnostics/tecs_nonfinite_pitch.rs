//! Non-finite TECS pitch integrator/setpoint detection.
//!
//! Monitors `tecs_status` for the pitch integrator (`pitch_integ`) or pitch
//! setpoint (`pitch_sp_rad`) becoming non-finite (NaN or +/-Inf). This is a
//! real, recurring, flight-critical failure: once the pitch integrator latches
//! a NaN it propagates into the pitch setpoint, the aircraft loses pitch
//! control, and it sheds altitude until a pilot intervenes.
//!
//! TECS is a low-rate fixed-wing/VTOL topic, so a single `is_finite()` check
//! per sample is cheap and fits inside the diagnostics budget.

use super::{
    parse_field, AnomalyKind, Analyzer, Diagnostic, Evidence, FieldUnit, OutputDescriptor,
    PlotAnchor, Severity, TecsNonfinitePitchField,
};
use px4_ulog::stream_parser::model::DataMessage;

const TOPIC: &str = "tecs_status";

pub struct TecsNonfinitePitchAnalyzer {
    /// Timestamp/field of the first non-finite sample, if any.
    first_nonfinite: Option<(u64, TecsNonfinitePitchField)>,
    /// Was the throttle integrator also non-finite at that sample?
    throttle_integ_nonfinite: bool,
    /// Last timestamp seen on the topic — becomes the region end.
    last_timestamp: u64,
}

impl Default for TecsNonfinitePitchAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl TecsNonfinitePitchAnalyzer {
    pub fn new() -> Self {
        Self {
            first_nonfinite: None,
            throttle_integ_nonfinite: false,
            last_timestamp: 0,
        }
    }
}

impl Analyzer for TecsNonfinitePitchAnalyzer {
    fn id(&self) -> &str {
        "tecs_nonfinite_pitch"
    }

    fn description(&self) -> &str {
        "Non-finite TECS pitch integrator or setpoint"
    }

    fn required_topics(&self) -> &[&str] {
        &[TOPIC]
    }

    fn on_message(&mut self, data: &DataMessage) {
        if data.flattened_format.message_name.as_str() != TOPIC {
            return;
        }

        let ts = data
            .flattened_format
            .timestamp_field
            .as_ref()
            .map(|tf| tf.parse_timestamp(data.data))
            .unwrap_or(0);
        self.last_timestamp = ts;

        // Once latched, keep tracking the end timestamp but don't re-detect.
        if self.first_nonfinite.is_some() {
            return;
        }

        let pitch_integ = parse_field::<f32>(data, "pitch_integ");
        let pitch_sp_rad = parse_field::<f32>(data, "pitch_sp_rad");

        // Prefer pitch_integ as the originating field, since the NaN latches
        // there first and propagates into the setpoint.
        let field = match (pitch_integ, pitch_sp_rad) {
            (Some(v), _) if !v.is_finite() => Some(TecsNonfinitePitchField::PitchInteg),
            (_, Some(v)) if !v.is_finite() => Some(TecsNonfinitePitchField::PitchSpRad),
            _ => None,
        };

        if let Some(field) = field {
            self.throttle_integ_nonfinite = parse_field::<f32>(data, "throttle_integ")
                .map(|v| !v.is_finite())
                .unwrap_or(false);
            self.first_nonfinite = Some((ts, field));
        }
    }

    fn finish(self: Box<Self>) -> Vec<Diagnostic> {
        let descriptor = self.output_descriptor();
        let Some((ts, field)) = self.first_nonfinite else {
            return vec![];
        };

        // Region runs from first detection to the last sample seen.
        let end = self.last_timestamp.max(ts);
        let field_name = match field {
            TecsNonfinitePitchField::PitchInteg => "pitch_integ",
            TecsNonfinitePitchField::PitchSpRad => "pitch_sp_rad",
        };

        vec![Diagnostic {
            id: "tecs_nonfinite_pitch".to_string(),
            summary: format!(
                "TECS {} became non-finite (NaN) at {:.1}s and never recovered — aircraft loses pitch control",
                field_name,
                ts as f64 / 1_000_000.0,
            ),
            severity: Severity::Critical,
            kind: AnomalyKind::Region { end_timestamp_us: end },
            timestamp_us: ts,
            anchor: PlotAnchor::new(TOPIC, "pitch_integ"),
            descriptor,
            evidence: Evidence::TecsNonfinitePitch {
                field,
                throttle_integ_nonfinite: self.throttle_integ_nonfinite,
            },
        }]
    }

    fn output_descriptor(&self) -> OutputDescriptor {
        OutputDescriptor::new()
            .field("field", FieldUnit::Label)
            .field("throttle_integ_nonfinite", FieldUnit::Label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::testing::*;

    fn tecs_msg(ts: u64, pitch_integ: f32, pitch_sp_rad: f32, throttle_integ: f32) -> (px4_ulog::stream_parser::model::FlattenedFormat, Vec<u8>) {
        MessageBuilder::new("tecs_status")
            .timestamp(ts)
            .field_f32("pitch_integ", pitch_integ)
            .field_f32("pitch_sp_rad", pitch_sp_rad)
            .field_f32("throttle_integ", throttle_integ)
            .build()
    }

    #[test]
    fn no_false_positives_sample() {
        assert_no_false_positives("sample.ulg", "tecs_nonfinite_pitch");
    }

    #[test]
    fn detects_nonfinite_pitch_integ() {
        let mut analyzer = TecsNonfinitePitchAnalyzer::new();

        // Healthy samples.
        for i in 0..5 {
            let (fmt, data) = tecs_msg((i + 1) * 100_000, 0.01, 0.02, 0.5);
            analyzer.on_message(&make_data_message(&fmt, &data));
        }
        // pitch_integ latches NaN.
        let (fmt, data) = tecs_msg(600_000, f32::NAN, 0.02, 0.5);
        analyzer.on_message(&make_data_message(&fmt, &data));
        // ... and stays bad to end of log.
        for i in 6..10 {
            let (fmt, data) = tecs_msg((i + 1) * 100_000, f32::NAN, f32::NAN, 0.5);
            analyzer.on_message(&make_data_message(&fmt, &data));
        }

        let diags = Box::new(analyzer).finish();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Critical);
        assert_eq!(diags[0].timestamp_us, 600_000);
        match &diags[0].kind {
            AnomalyKind::Region { end_timestamp_us } => assert_eq!(*end_timestamp_us, 1_000_000),
            _ => panic!("expected Region"),
        }
        match &diags[0].evidence {
            Evidence::TecsNonfinitePitch { field, .. } => {
                assert_eq!(*field, TecsNonfinitePitchField::PitchInteg)
            }
            _ => panic!("expected TecsNonfinitePitch"),
        }
    }

    #[test]
    fn detects_nonfinite_pitch_setpoint() {
        let mut analyzer = TecsNonfinitePitchAnalyzer::new();
        let (fmt, data) = tecs_msg(100_000, 0.01, f32::INFINITY, 0.5);
        analyzer.on_message(&make_data_message(&fmt, &data));

        let diags = Box::new(analyzer).finish();
        assert_eq!(diags.len(), 1);
        match &diags[0].evidence {
            Evidence::TecsNonfinitePitch { field, .. } => {
                assert_eq!(*field, TecsNonfinitePitchField::PitchSpRad)
            }
            _ => panic!("expected TecsNonfinitePitch"),
        }
    }

    #[test]
    fn fires_once_not_per_sample() {
        let mut analyzer = TecsNonfinitePitchAnalyzer::new();
        for i in 0..20 {
            let (fmt, data) = tecs_msg((i + 1) * 100_000, f32::NAN, f32::NAN, 0.5);
            analyzer.on_message(&make_data_message(&fmt, &data));
        }
        let diags = Box::new(analyzer).finish();
        assert_eq!(diags.len(), 1, "should fire exactly once for the first NaN");
    }

    #[test]
    fn captures_throttle_integ_sibling() {
        let mut analyzer = TecsNonfinitePitchAnalyzer::new();
        let (fmt, data) = tecs_msg(100_000, f32::NAN, 0.02, f32::NAN);
        analyzer.on_message(&make_data_message(&fmt, &data));
        let diags = Box::new(analyzer).finish();
        match &diags[0].evidence {
            Evidence::TecsNonfinitePitch { throttle_integ_nonfinite, .. } => {
                assert!(*throttle_integ_nonfinite)
            }
            _ => panic!("expected TecsNonfinitePitch"),
        }
    }

    #[test]
    fn no_detection_when_finite() {
        let mut analyzer = TecsNonfinitePitchAnalyzer::new();
        for i in 0..20 {
            let (fmt, data) = tecs_msg((i + 1) * 100_000, 0.01, 0.02, 0.5);
            analyzer.on_message(&make_data_message(&fmt, &data));
        }
        let diags = Box::new(analyzer).finish();
        assert!(diags.is_empty());
    }

    #[test]
    fn handles_missing_fields() {
        let mut analyzer = TecsNonfinitePitchAnalyzer::new();
        let (fmt, data) = MessageBuilder::new("tecs_status")
            .timestamp(1_000_000)
            .build();
        analyzer.on_message(&make_data_message(&fmt, &data)); // must not panic
        let diags = Box::new(analyzer).finish();
        assert!(diags.is_empty());
    }

    #[test]
    fn snapshot_sample_ulg() {
        let diags = analyze_fixture_for("sample.ulg", "tecs_nonfinite_pitch");
        insta::assert_json_snapshot!(diags);
    }

    #[test]
    fn detects_real_tecs_nonfinite_pitch() {
        // Real PX4 FMU_V6X fixed-wing log where the TECS pitch integrator
        // latched NaN in flight. Sourced from the public PX4 log corpus.
        let diags = analyze_fixture_for("tecs_nonfinite_pitch.ulg", "tecs_nonfinite_pitch");
        assert_eq!(
            diags.len(),
            1,
            "should detect exactly one non-finite TECS pitch event"
        );
        assert_eq!(diags[0].severity, Severity::Critical);
        assert!(matches!(diags[0].kind, AnomalyKind::Region { .. }));
        match &diags[0].evidence {
            Evidence::TecsNonfinitePitch { field, .. } => {
                assert_eq!(*field, TecsNonfinitePitchField::PitchInteg)
            }
            _ => panic!("expected TecsNonfinitePitch evidence"),
        }
        insta::assert_json_snapshot!(diags);
    }
}
