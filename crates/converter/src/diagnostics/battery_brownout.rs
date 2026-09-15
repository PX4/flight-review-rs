//! Battery brownout detection analyzer.
//!
//! Reports low voltage on a connected battery while armed, using its reported
//! cell count. This threshold is a heuristic, not evidence of a power-rail
//! brownout. Unknown cell count/connection state is not inferred from voltage.
//!
//! NEGATIVE_FIXTURE: battery_brownout.ulg is a disconnected 4S sensor, not a
//! demonstrated brownout. A labeled positive flight fixture is still needed.

use super::{
    parse_bool, parse_field, Analyzer, AnomalyKind, Diagnostic, Evidence, FieldUnit,
    OutputDescriptor, PlotAnchor, Severity,
};
use px4_ulog::stream_parser::model::DataMessage;

/// Per-cell critical voltage threshold (V).
const CRITICAL_VOLTAGE_PER_CELL: f32 = 3.3;
/// Minimum time (microseconds) between detections.
/// Set high to avoid flooding — one detection per brownout event is enough.
const DEDUP_INTERVAL_US: u64 = 30_000_000;

pub struct BatteryBrownoutAnalyzer {
    armed: bool,
    cell_count: Option<u8>,
    critical_threshold_v: f32,
    last_detection_us: Option<u64>,
    last_sample_us: Option<u64>,
    detections: Vec<Diagnostic>,
}

impl Default for BatteryBrownoutAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl BatteryBrownoutAnalyzer {
    pub fn new() -> Self {
        Self {
            armed: false,
            cell_count: None,
            critical_threshold_v: 0.0,
            last_detection_us: None,
            last_sample_us: None,
            detections: Vec::new(),
        }
    }
}

impl Analyzer for BatteryBrownoutAnalyzer {
    fn id(&self) -> &str {
        "battery_brownout"
    }

    fn description(&self) -> &str {
        "Voltage below critical threshold"
    }

    fn required_topics(&self) -> &[&str] {
        &["battery_status", "vehicle_status"]
    }

    fn on_message(&mut self, data: &DataMessage) {
        let topic = data.flattened_format.message_name.as_str();

        match topic {
            "vehicle_status" => {
                if let Some(arming) = parse_field::<u8>(data, "arming_state") {
                    self.armed = arming == 2;
                    if !self.armed {
                        self.last_detection_us = None;
                    }
                }
            }
            "battery_status" => {
                let Some(ts) = super::timestamp(data) else {
                    return;
                };
                if self.last_sample_us.is_some_and(|previous| ts <= previous) {
                    return;
                }
                self.last_sample_us = Some(ts);
                if parse_bool(data, "connected") != Some(true) {
                    self.cell_count = None;
                    self.last_detection_us = None;
                    return;
                }
                let Some(cells) = parse_field::<u8>(data, "cell_count").filter(|cells| *cells > 0)
                else {
                    self.cell_count = None;
                    return;
                };
                let Some(voltage) = parse_field::<f32>(data, "voltage_v")
                    .filter(|v| v.is_finite() && *v > 0.0)
                    .or_else(|| parse_field::<f32>(data, "voltage_filtered_v"))
                else {
                    return;
                };

                if !voltage.is_finite() || voltage <= 0.0 {
                    return;
                }

                if self.cell_count != Some(cells) {
                    self.last_detection_us = None;
                }
                self.cell_count = Some(cells);
                self.critical_threshold_v = cells as f32 * CRITICAL_VOLTAGE_PER_CELL;

                if !self.armed {
                    return;
                }

                // Deduplication
                if self
                    .last_detection_us
                    .is_some_and(|last| ts.saturating_sub(last) < DEDUP_INTERVAL_US)
                {
                    return;
                }

                if voltage < self.critical_threshold_v {
                    let current = parse_field::<f32>(data, "current_a")
                        .or_else(|| parse_field::<f32>(data, "current_filtered_a"))
                        .filter(|current| current.is_finite() && *current >= 0.0);

                    self.last_detection_us = Some(ts);
                    self.detections.push(Diagnostic {
                        id: "battery_brownout".to_string(),
                        summary: format!(
                            "Battery voltage {:.2}V below critical threshold {:.1}V at {:.1}s ({}S)",
                            voltage,
                            self.critical_threshold_v,
                            ts as f64 / 1_000_000.0,
                            self.cell_count.unwrap_or(0),
                        ),
                        severity: Severity::Critical,
                        kind: AnomalyKind::Point,
                        timestamp_us: ts,
                        anchor: PlotAnchor::new("battery_status", "voltage_v"),
                        descriptor: self.output_descriptor(),
                        evidence: Evidence::BatteryBrownout {
                            voltage_v: voltage,
                            critical_threshold_v: self.critical_threshold_v,
                            current_a: current,
                        },
                    });
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
            .field("voltage_v", FieldUnit::Volts)
            .field("critical_threshold_v", FieldUnit::Volts)
            .field("current_a", FieldUnit::Amps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::testing::*;

    #[test]
    fn no_false_positives_sample() {
        assert_no_false_positives("sample.ulg", "battery_brownout");
    }

    #[test]
    fn detects_low_voltage() {
        let mut analyzer = BatteryBrownoutAnalyzer::new();

        // Arm
        let (fmt, data) = MessageBuilder::new("vehicle_status")
            .timestamp(1_000_000)
            .field_u8("arming_state", 2)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm);

        // Initial battery reading at ~4S (16.8V full)
        let (fmt2, data2) = MessageBuilder::new("battery_status")
            .timestamp(2_000_000)
            .field_f32("voltage_v", 16.0)
            .field_bool("connected", true)
            .field_u8("cell_count", 4)
            .field_f32("current_a", 10.0)
            .build();
        let dm2 = make_data_message(&fmt2, &data2);
        analyzer.on_message(&dm2);

        // Voltage drops below critical (4 * 3.3 = 13.2V)
        let (fmt3, data3) = MessageBuilder::new("battery_status")
            .timestamp(40_000_000)
            .field_f32("voltage_v", 12.5)
            .field_bool("connected", true)
            .field_u8("cell_count", 4)
            .field_f32("current_a", 15.0)
            .build();
        let dm3 = make_data_message(&fmt3, &data3);
        analyzer.on_message(&dm3);

        let diags = Box::new(analyzer).finish();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Critical);
        match &diags[0].evidence {
            Evidence::BatteryBrownout {
                voltage_v,
                critical_threshold_v,
                current_a,
            } => {
                assert!(*voltage_v < *critical_threshold_v);
                assert_eq!(*current_a, Some(15.0));
            }
            _ => panic!("Expected BatteryBrownout evidence"),
        }
    }

    #[test]
    fn no_detection_when_disarmed() {
        let mut analyzer = BatteryBrownoutAnalyzer::new();

        // Stay disarmed
        let (fmt, data) = MessageBuilder::new("vehicle_status")
            .timestamp(1_000_000)
            .field_u8("arming_state", 0)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm);

        // Initial reading
        let (fmt2, data2) = MessageBuilder::new("battery_status")
            .timestamp(2_000_000)
            .field_f32("voltage_v", 16.0)
            .field_bool("connected", true)
            .field_u8("cell_count", 4)
            .build();
        let dm2 = make_data_message(&fmt2, &data2);
        analyzer.on_message(&dm2);

        // Low voltage while disarmed
        let (fmt3, data3) = MessageBuilder::new("battery_status")
            .timestamp(10_000_000)
            .field_f32("voltage_v", 10.0)
            .field_bool("connected", true)
            .field_u8("cell_count", 4)
            .build();
        let dm3 = make_data_message(&fmt3, &data3);
        analyzer.on_message(&dm3);

        let diags = Box::new(analyzer).finish();
        assert!(diags.is_empty());
    }

    #[test]
    fn handles_missing_fields() {
        let mut analyzer = BatteryBrownoutAnalyzer::new();

        let (fmt, data) = MessageBuilder::new("battery_status")
            .timestamp(1_000_000)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm); // must not panic

        let diags = Box::new(analyzer).finish();
        assert!(diags.is_empty());
    }

    #[test]
    fn deduplicates_within_interval() {
        let mut analyzer = BatteryBrownoutAnalyzer::new();

        let (fmt, data) = MessageBuilder::new("vehicle_status")
            .timestamp(1_000_000)
            .field_u8("arming_state", 2)
            .build();
        let dm = make_data_message(&fmt, &data);
        analyzer.on_message(&dm);

        // Initial reading
        let (fmt2, data2) = MessageBuilder::new("battery_status")
            .timestamp(2_000_000)
            .field_f32("voltage_v", 16.0)
            .field_bool("connected", true)
            .field_u8("cell_count", 4)
            .build();
        let dm2 = make_data_message(&fmt2, &data2);
        analyzer.on_message(&dm2);

        // Two low readings within dedup interval (30s)
        for ts in [40_000_000u64, 50_000_000] {
            let (fmt3, data3) = MessageBuilder::new("battery_status")
                .timestamp(ts)
                .field_f32("voltage_v", 12.0)
                .field_bool("connected", true)
                .field_u8("cell_count", 4)
                .build();
            let dm3 = make_data_message(&fmt3, &data3);
            analyzer.on_message(&dm3);
        }

        let diags = Box::new(analyzer).finish();
        assert_eq!(diags.len(), 1, "Should deduplicate within 30s interval");
    }

    #[test]
    fn reported_cells_detect_depleted_pack_before_thirty_seconds() {
        let mut analyzer = BatteryBrownoutAnalyzer::new();
        feed(
            &mut analyzer,
            MessageBuilder::new("vehicle_status")
                .timestamp(0)
                .field_u8("arming_state", 2),
            0,
        );
        for (ts, voltage) in [(1_000_000, 21.0), (2_000_000, 18.0)] {
            feed(
                &mut analyzer,
                MessageBuilder::new("battery_status")
                    .timestamp(ts)
                    .field_bool("connected", true)
                    .field_u8("cell_count", 6)
                    .field_f32("voltage_v", voltage),
                0,
            );
        }
        let diags = Box::new(analyzer).finish();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].timestamp_us, 2_000_000);
        assert!(matches!(diags[0].evidence, Evidence::BatteryBrownout {
            critical_threshold_v, .. } if (critical_threshold_v - 19.8).abs() < 0.001));
    }

    #[test]
    fn snapshot_sample_ulg() {
        let diags = analyze_fixture_for("sample.ulg", "battery_brownout");
        insta::assert_json_snapshot!(diags);
    }

    #[test]
    fn no_false_positives_disconnected_fixture() {
        let diags = analyze_fixture_for("battery_brownout.ulg", "battery_brownout");
        // All 382 readings report connected=false and cell_count=4. The
        // 0..0.133V ADC noise is not evidence of a 1S battery brownout.
        assert!(diags.is_empty());
        insta::assert_json_snapshot!(diags);
    }
}
