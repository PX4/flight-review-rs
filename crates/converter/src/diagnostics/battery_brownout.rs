//! Battery brownout detection analyzer.
//!
//! Detects dangerously low battery voltage during armed flight. Auto-detects
//! cell count from initial voltage and flags when voltage drops below
//! critical threshold per cell.

use super::{parse_field, Analyzer, Diagnostic, Evidence, Severity};
use px4_ulog::stream_parser::model::DataMessage;

/// Per-cell critical voltage threshold (V).
const CRITICAL_VOLTAGE_PER_CELL: f32 = 3.3;
/// Nominal full-charge voltage per cell for cell count estimation.
const NOMINAL_FULL_VOLTAGE_PER_CELL: f32 = 4.2;
/// Minimum time (microseconds) between detections.
/// Set high to avoid flooding — one detection per brownout event is enough.
const DEDUP_INTERVAL_US: u64 = 30_000_000;

pub struct BatteryBrownoutAnalyzer {
    armed: bool,
    cell_count: Option<u8>,
    critical_threshold_v: f32,
    last_detection_us: u64,
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
            last_detection_us: 0,
            detections: Vec::new(),
        }
    }

    fn estimate_cell_count(voltage: f32) -> u8 {
        let cells = (voltage / NOMINAL_FULL_VOLTAGE_PER_CELL).round() as u8;
        cells.clamp(1, 12)
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
                }
            }
            "battery_status" => {
                let Some(voltage) = parse_field::<f32>(data, "voltage_v")
                    .or_else(|| parse_field::<f32>(data, "voltage_filtered_v"))
                else {
                    return;
                };

                if !voltage.is_finite() || voltage <= 0.0 {
                    return;
                }

                // Estimate cell count from first reading
                if self.cell_count.is_none() {
                    let cells = Self::estimate_cell_count(voltage);
                    self.cell_count = Some(cells);
                    self.critical_threshold_v = cells as f32 * CRITICAL_VOLTAGE_PER_CELL;
                }

                if !self.armed {
                    return;
                }

                let ts = data
                    .flattened_format
                    .timestamp_field
                    .as_ref()
                    .map(|tf| tf.parse_timestamp(data.data))
                    .unwrap_or(0);

                // Deduplication
                if ts > 0 && ts - self.last_detection_us < DEDUP_INTERVAL_US {
                    return;
                }

                if voltage < self.critical_threshold_v {
                    let current = parse_field::<f32>(data, "current_a")
                        .or_else(|| parse_field::<f32>(data, "current_filtered_a"));

                    self.last_detection_us = ts;
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
                        timestamp_us: ts,
                        end_timestamp_us: None,
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
            .field_f32("current_a", 10.0)
            .build();
        let dm2 = make_data_message(&fmt2, &data2);
        analyzer.on_message(&dm2);

        // Voltage drops below critical (4 * 3.3 = 13.2V)
        let (fmt3, data3) = MessageBuilder::new("battery_status")
            .timestamp(40_000_000)
            .field_f32("voltage_v", 12.5)
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
            .build();
        let dm2 = make_data_message(&fmt2, &data2);
        analyzer.on_message(&dm2);

        // Low voltage while disarmed
        let (fmt3, data3) = MessageBuilder::new("battery_status")
            .timestamp(10_000_000)
            .field_f32("voltage_v", 10.0)
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
            .build();
        let dm2 = make_data_message(&fmt2, &data2);
        analyzer.on_message(&dm2);

        // Two low readings within dedup interval (30s)
        for ts in [40_000_000u64, 50_000_000] {
            let (fmt3, data3) = MessageBuilder::new("battery_status")
                .timestamp(ts)
                .field_f32("voltage_v", 12.0)
                .build();
            let dm3 = make_data_message(&fmt3, &data3);
            analyzer.on_message(&dm3);
        }

        let diags = Box::new(analyzer).finish();
        assert_eq!(diags.len(), 1, "Should deduplicate within 2s interval");
    }

    #[test]
    fn cell_count_estimation() {
        assert_eq!(BatteryBrownoutAnalyzer::estimate_cell_count(16.8), 4);
        assert_eq!(BatteryBrownoutAnalyzer::estimate_cell_count(12.6), 3);
        assert_eq!(BatteryBrownoutAnalyzer::estimate_cell_count(25.2), 6);
        assert_eq!(BatteryBrownoutAnalyzer::estimate_cell_count(4.2), 1);
    }

    #[test]
    fn snapshot_sample_ulg() {
        let diags = analyze_fixture_for("sample.ulg", "battery_brownout");
        insta::assert_json_snapshot!(diags);
    }

    #[test]
    fn detects_real_battery_brownout() {
        let diags = analyze_fixture_for("battery_brownout.ulg", "battery_brownout");
        assert!(
            !diags.is_empty(),
            "Should detect brownout in real low-voltage log"
        );
        assert!(
            diags.iter().all(|d| d.severity == Severity::Critical),
            "All brownout detections should be critical"
        );
        insta::assert_json_snapshot!(diags);
    }
}
