//! GPS quality degradation (legacy ID: gps_interference).
//! Accuracy/satellite changes do not identify interference as their cause.
//!
//! NEGATIVE_FIXTURE: gps_interference.ulg has no fix throughout, not an observed
//! good-to-bad transition. A labeled positive degradation fixture is needed.
//!
//! Monitors `vehicle_gps_position` for sudden degradation in GPS quality:
//! - EPH (horizontal position error) spikes above baseline
//! - EPV (vertical position error) spikes above threshold
//! - Satellite count drops significantly below baseline

use super::{
    parse_field, Analyzer, AnomalyKind, Diagnostic, Evidence, FieldUnit, OutputDescriptor,
    PlotAnchor, Severity,
};
use px4_ulog::stream_parser::model::DataMessage;

/// Number of samples to establish baseline.
const BASELINE_SAMPLES: u32 = 10;
/// EPH threshold for detection when baseline is good.
const EPH_SPIKE_THRESHOLD: f32 = 5.0;
/// EPH threshold for critical severity.
const EPH_CRITICAL_THRESHOLD: f32 = 10.0;
/// EPV threshold for detection.
const EPV_THRESHOLD: f32 = 10.0;
/// Minimum satellite count for critical severity.
const SATS_CRITICAL_MIN: u16 = 4;
/// Satellite drop percentage threshold (0.0 to 1.0).
const SATS_DROP_RATIO: f64 = 0.5;
/// Minimum time (microseconds) between detections to avoid duplicates.
const DEDUP_INTERVAL_US: u64 = 5_000_000;

pub struct GpsInterferenceAnalyzer {
    // Baseline accumulation
    sample_count: u32,
    eph_sum: f64,
    sats_sum: f64,
    baseline_eph: Option<f32>,
    baseline_sats: Option<f64>,
    // Deduplication
    last_detection_us: Option<u64>,
    last_sample_us: Option<u64>,
    detections: Vec<Diagnostic>,
}

impl Default for GpsInterferenceAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl GpsInterferenceAnalyzer {
    pub fn new() -> Self {
        Self {
            sample_count: 0,
            eph_sum: 0.0,
            sats_sum: 0.0,
            baseline_eph: None,
            baseline_sats: None,
            last_detection_us: None,
            last_sample_us: None,
            detections: Vec::new(),
        }
    }
}

impl Analyzer for GpsInterferenceAnalyzer {
    fn id(&self) -> &str {
        "gps_interference"
    }

    fn description(&self) -> &str {
        "EPH/EPV spike, satellite count drop"
    }

    fn required_topics(&self) -> &[&str] {
        &["vehicle_gps_position"]
    }

    fn on_message(&mut self, data: &DataMessage) {
        let topic = data.flattened_format.message_name.as_str();
        if topic != "vehicle_gps_position" {
            return;
        }

        let Some(ts) = super::timestamp(data) else {
            return;
        };
        if self.last_sample_us.is_some_and(|previous| ts <= previous) {
            return;
        }
        self.last_sample_us = Some(ts);
        let (Some(current_eph), Some(current_epv), Some(current_sats)) = (
            parse_field::<f32>(data, "eph").filter(|v| v.is_finite() && *v >= 0.0),
            parse_field::<f32>(data, "epv").filter(|v| v.is_finite() && *v >= 0.0),
            parse_field::<u8>(data, "satellites_used").map(u16::from),
        ) else {
            return;
        };
        let fix_valid = parse_field::<u8>(data, "fix_type").is_none_or(|fix| fix >= 3);

        // Accumulate baseline from first N samples
        if self.sample_count < BASELINE_SAMPLES {
            if !fix_valid
                || current_sats < SATS_CRITICAL_MIN
                || current_eph <= 0.0
                || current_epv <= 0.0
            {
                // Establish a baseline only from consecutive usable fixes.
                self.sample_count = 0;
                self.eph_sum = 0.0;
                self.sats_sum = 0.0;
                return;
            }
            self.eph_sum += current_eph as f64;
            self.sats_sum += current_sats as f64;
            self.sample_count += 1;

            if self.sample_count == BASELINE_SAMPLES {
                self.baseline_eph = Some((self.eph_sum / BASELINE_SAMPLES as f64) as f32);
                self.baseline_sats = Some(self.sats_sum / BASELINE_SAMPLES as f64);
            }
            return;
        }

        // Only detect after baseline is established
        let baseline_eph = match self.baseline_eph {
            Some(b) => b,
            None => return,
        };
        let baseline_sats = self.baseline_sats.unwrap_or(0.0);

        // Deduplication check
        if self
            .last_detection_us
            .is_some_and(|last| ts.saturating_sub(last) < DEDUP_INTERVAL_US)
        {
            return;
        }

        // EPH spike detection
        let eph_spike = current_eph > EPH_SPIKE_THRESHOLD && current_eph > 3.0 * baseline_eph;
        // EPV spike detection
        let epv_spike = current_epv > EPV_THRESHOLD && current_epv.is_finite();
        // Satellite drop detection
        let sats_drop =
            baseline_sats > 0.0 && (current_sats as f64) < baseline_sats * (1.0 - SATS_DROP_RATIO);

        if eph_spike || epv_spike || sats_drop {
            let severity =
                if current_eph > EPH_CRITICAL_THRESHOLD || current_sats < SATS_CRITICAL_MIN {
                    Severity::Critical
                } else {
                    Severity::Warning
                };

            let mut reasons = Vec::new();
            if eph_spike {
                reasons.push(format!(
                    "EPH {:.1}m (baseline {:.1}m)",
                    current_eph, baseline_eph
                ));
            }
            if epv_spike {
                reasons.push(format!("EPV {:.1}m", current_epv));
            }
            if sats_drop {
                reasons.push(format!(
                    "satellites {} (baseline {:.0})",
                    current_sats, baseline_sats
                ));
            }

            self.last_detection_us = Some(ts);
            self.detections.push(Diagnostic {
                id: "gps_interference".to_string(),
                summary: format!(
                    "GPS quality degraded at {:.1}s: {}",
                    ts as f64 / 1_000_000.0,
                    reasons.join(", ")
                ),
                severity,
                kind: AnomalyKind::Point,
                timestamp_us: ts,
                anchor: PlotAnchor::new("vehicle_gps_position", "eph"),
                descriptor: self.output_descriptor(),
                evidence: Evidence::GpsInterference {
                    eph_m: current_eph,
                    epv_m: current_epv,
                    num_satellites: current_sats,
                    noise_level: None,
                },
            });
        }
    }

    fn finish(self: Box<Self>) -> Vec<Diagnostic> {
        self.detections
    }

    fn output_descriptor(&self) -> OutputDescriptor {
        OutputDescriptor::new()
            .field("eph_m", FieldUnit::Meters)
            .field("epv_m", FieldUnit::Meters)
            .field("num_satellites", FieldUnit::Count)
            .field("noise_level", FieldUnit::Ratio)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::testing::*;

    #[test]
    fn no_false_positives_sample() {
        assert_no_false_positives("sample.ulg", "gps_interference");
    }

    #[test]
    fn detects_eph_spike() {
        let mut analyzer = GpsInterferenceAnalyzer::new();

        // Feed baseline samples with good EPH
        for i in 0..BASELINE_SAMPLES {
            let (fmt, data) = MessageBuilder::new("vehicle_gps_position")
                .timestamp((i as u64 + 1) * 1_000_000)
                .field_f32("eph", 1.0)
                .field_f32("epv", 1.0)
                .field_u8("satellites_used", 12)
                .build();
            let dm = make_data_message(&fmt, &data);
            analyzer.on_message(&dm);
        }

        // Spike EPH
        let (fmt, data) = MessageBuilder::new("vehicle_gps_position")
            .timestamp(20_000_000)
            .field_f32("eph", 8.0)
            .field_f32("epv", 2.0)
            .field_u8("satellites_used", 12)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm);

        let diags = Box::new(analyzer).finish();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Warning);
    }

    #[test]
    fn detects_critical_eph() {
        let mut analyzer = GpsInterferenceAnalyzer::new();

        for i in 0..BASELINE_SAMPLES {
            let (fmt, data) = MessageBuilder::new("vehicle_gps_position")
                .timestamp((i as u64 + 1) * 1_000_000)
                .field_f32("eph", 1.0)
                .field_f32("epv", 1.0)
                .field_u8("satellites_used", 12)
                .build();
            let dm = make_data_message(&fmt, &data);
            analyzer.on_message(&dm);
        }

        // Critical EPH spike
        let (fmt, data) = MessageBuilder::new("vehicle_gps_position")
            .timestamp(20_000_000)
            .field_f32("eph", 15.0)
            .field_f32("epv", 2.0)
            .field_u8("satellites_used", 12)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm);

        let diags = Box::new(analyzer).finish();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Critical);
    }

    #[test]
    fn detects_satellite_drop() {
        let mut analyzer = GpsInterferenceAnalyzer::new();

        for i in 0..BASELINE_SAMPLES {
            let (fmt, data) = MessageBuilder::new("vehicle_gps_position")
                .timestamp((i as u64 + 1) * 1_000_000)
                .field_f32("eph", 1.0)
                .field_f32("epv", 1.0)
                .field_u8("satellites_used", 12)
                .build();
            let dm = make_data_message(&fmt, &data);
            analyzer.on_message(&dm);
        }

        // Satellite drop to 3 (critical)
        let (fmt, data) = MessageBuilder::new("vehicle_gps_position")
            .timestamp(20_000_000)
            .field_f32("eph", 1.5)
            .field_f32("epv", 1.5)
            .field_u8("satellites_used", 3)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm);

        let diags = Box::new(analyzer).finish();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Critical);
    }

    #[test]
    fn handles_missing_fields() {
        let mut analyzer = GpsInterferenceAnalyzer::new();

        let (fmt, data) = MessageBuilder::new("vehicle_gps_position")
            .timestamp(1_000_000)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm); // must not panic

        let diags = Box::new(analyzer).finish();
        assert!(diags.is_empty());
    }

    #[test]
    fn deduplicates_within_interval() {
        let mut analyzer = GpsInterferenceAnalyzer::new();

        for i in 0..BASELINE_SAMPLES {
            let (fmt, data) = MessageBuilder::new("vehicle_gps_position")
                .timestamp((i as u64 + 1) * 1_000_000)
                .field_f32("eph", 1.0)
                .field_f32("epv", 1.0)
                .field_u8("satellites_used", 12)
                .build();
            let dm = make_data_message(&fmt, &data);
            analyzer.on_message(&dm);
        }

        // Two spikes within dedup interval
        for ts in [20_000_000u64, 22_000_000] {
            let (fmt, data) = MessageBuilder::new("vehicle_gps_position")
                .timestamp(ts)
                .field_f32("eph", 8.0)
                .field_f32("epv", 2.0)
                .field_u8("satellites_used", 12)
                .build();
            let dm = make_data_message(&fmt, &data);
            analyzer.on_message(&dm);
        }

        let diags = Box::new(analyzer).finish();
        assert_eq!(diags.len(), 1, "Should deduplicate within 5s interval");
    }

    #[test]
    fn snapshot_sample_ulg() {
        let diags = analyze_fixture_for("sample.ulg", "gps_interference");
        insta::assert_json_snapshot!(diags);
    }

    #[test]
    fn no_false_positives_no_fix_fixture() {
        let diags = analyze_fixture_for("gps_interference.ulg", "gps_interference");
        // Every sample has fix_type=0 and satellites_used=0. It cannot
        // establish a valid baseline or prove interference caused a loss.
        assert!(diags.is_empty());
        insta::assert_json_snapshot!(diags);
    }
}
