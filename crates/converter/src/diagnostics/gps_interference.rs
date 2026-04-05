//! GPS interference detection analyzer.
//!
//! Monitors `vehicle_gps_position` for sudden degradation in GPS quality:
//! - EPH (horizontal position error) spikes above baseline
//! - EPV (vertical position error) spikes above threshold
//! - Satellite count drops significantly below baseline

use super::{parse_field, Analyzer, Diagnostic, Evidence, Severity};
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
    last_detection_us: u64,
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
            last_detection_us: 0,
            detections: Vec::new(),
        }
    }
}

impl Analyzer for GpsInterferenceAnalyzer {
    fn required_topics(&self) -> &[&str] {
        &["vehicle_gps_position"]
    }

    fn on_message(&mut self, data: &DataMessage) {
        let topic = data.flattened_format.message_name.as_str();
        if topic != "vehicle_gps_position" {
            return;
        }

        let ts = data
            .flattened_format
            .timestamp_field
            .as_ref()
            .map(|tf| tf.parse_timestamp(data.data))
            .unwrap_or(0);

        let eph = parse_field::<f32>(data, "eph");
        let epv = parse_field::<f32>(data, "epv");
        let sats = parse_field::<u8>(data, "satellites_used").map(|s| s as u16);

        // Accumulate baseline from first N samples
        if self.sample_count < BASELINE_SAMPLES {
            if let Some(e) = eph {
                if e.is_finite() {
                    self.eph_sum += e as f64;
                }
            }
            if let Some(s) = sats {
                self.sats_sum += s as f64;
            }
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
        if ts > 0 && ts - self.last_detection_us < DEDUP_INTERVAL_US {
            return;
        }

        let current_eph = eph.unwrap_or(0.0);
        let current_epv = epv.unwrap_or(0.0);
        let current_sats = sats.unwrap_or(0);

        // EPH spike detection
        let eph_spike = baseline_eph < 2.0 && current_eph > EPH_SPIKE_THRESHOLD && current_eph.is_finite();
        // EPV spike detection
        let epv_spike = current_epv > EPV_THRESHOLD && current_epv.is_finite();
        // Satellite drop detection
        let sats_drop = baseline_sats > 0.0
            && (current_sats as f64) < baseline_sats * (1.0 - SATS_DROP_RATIO);

        if eph_spike || epv_spike || sats_drop {
            let severity = if current_eph > EPH_CRITICAL_THRESHOLD
                || current_sats < SATS_CRITICAL_MIN
            {
                Severity::Critical
            } else {
                Severity::Warning
            };

            let mut reasons = Vec::new();
            if eph_spike {
                reasons.push(format!("EPH {:.1}m (baseline {:.1}m)", current_eph, baseline_eph));
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

            self.last_detection_us = ts;
            self.detections.push(Diagnostic {
                id: "gps_interference".to_string(),
                summary: format!(
                    "GPS quality degraded at {:.1}s: {}",
                    ts as f64 / 1_000_000.0,
                    reasons.join(", ")
                ),
                severity,
                timestamp_us: ts,
                end_timestamp_us: None,
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
}
