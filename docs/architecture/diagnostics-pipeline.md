# Diagnostics Pipeline Architecture

## Status: Proposal

## Problem

Flight Review v2 needs a way for researchers and contributors to add automated
flight diagnostics — detecting motor failures, GPS interference, vibration
anomalies, etc. — without compromising the project's architecture or
introducing external runtime dependencies.

## Constraints

1. **Rust-native.** All analysis runs inside the converter crate. No Python,
   no sidecar services, no additional runtimes.
2. **Inline with upload.** Diagnostics execute during the existing
   `spawn_blocking` conversion task. No background job queue.
3. **Streaming-friendly.** Analyzers consume ULog data via the streaming
   parser, same as `analysis.rs` and `pid_analysis.rs` do today.
4. **Zero false-positive tolerance for heuristics.** Deterministic rules must
   have clear physical justification. Statistical models are opt-in and clearly
   labeled as experimental.

## Current Processing Pipeline

```
convert_ulog()                          // crates/converter/src/converter.rs
├── px4_ulog::full_parser::read_file()  // parse ULog → in-memory topics
├── extract_metadata()                  // 1st streaming pass → FlightMetadata
├── analyze()                           // 2nd streaming pass → FlightAnalysis
└── write per-topic Parquet files
```

Results are stored as `metadata.json` alongside Parquet files, and key fields
are promoted to DB columns for indexed search.

## Proposed Extension: Diagnostic Analyzers

### Where It Fits

A new `diagnostics` module in the converter crate, called after `analyze()`:

```
convert_ulog()
├── extract_metadata()
├── analyze()
├── run_diagnostics()                   // NEW — 3rd streaming pass
└── write Parquet files
```

### The Analyzer Trait

```rust
// crates/converter/src/diagnostics/mod.rs

/// Severity of a detected anomaly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Severity {
    /// Informational — notable but not a problem.
    Info,
    /// Warning — potential issue, worth investigating.
    Warning,
    /// Critical — likely hardware failure or dangerous condition.
    Critical,
}

/// A single detected anomaly with evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Machine-readable identifier, e.g. "motor_failure", "gps_interference".
    pub id: String,
    /// Human-readable summary.
    pub summary: String,
    /// Severity classification.
    pub severity: Severity,
    /// Timestamp (microseconds) where the anomaly was first detected.
    pub timestamp_us: u64,
    /// Optional end timestamp if the anomaly spans a window.
    pub end_timestamp_us: Option<u64>,
    /// Key-value evidence (e.g. {"motor_index": "2", "pwm_value": "0"}).
    pub evidence: HashMap<String, String>,
}

/// Result of running all diagnostics on a single flight.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiagnosticsResult {
    pub diagnostics: Vec<Diagnostic>,
}

/// Trait that all diagnostic analyzers implement.
pub trait Analyzer {
    /// Which ULog topics this analyzer needs.
    /// The runner will only subscribe to topics required by active analyzers.
    fn required_topics(&self) -> &[&str];

    /// Called once per data message for a subscribed topic.
    /// `topic` is the topic name, `timestamp_us` is the message timestamp,
    /// `data` is the raw message bytes, and `fields` provides the field layout.
    fn on_message(
        &mut self,
        topic: &str,
        timestamp_us: u64,
        data: &[u8],
        fields: &[FlattenedField],
    );

    /// Called after the streaming pass completes. Return any detected anomalies.
    fn finish(self) -> Vec<Diagnostic>;
}
```

### The Runner

```rust
/// Run all registered analyzers over the ULog file in a single streaming pass.
pub fn run_diagnostics(
    path: &str,
    metadata: &FlightMetadata,
    analysis: &FlightAnalysis,
) -> Result<DiagnosticsResult, std::io::Error> {
    // Build the set of analyzers
    let mut analyzers: Vec<Box<dyn Analyzer>> = vec![
        Box::new(MotorFailureAnalyzer::new(metadata)),
        Box::new(GpsInterferenceAnalyzer::new(metadata)),
        Box::new(VibrationAnomalyAnalyzer::new(metadata, analysis)),
        // Contributors add new analyzers here
    ];

    // Collect required topics across all analyzers
    let required: HashSet<&str> = analyzers
        .iter()
        .flat_map(|a| a.required_topics())
        .collect();

    // Single streaming pass — dispatch messages to relevant analyzers
    read_file_with_simple_callback(path, |msg| {
        if let Message::Data { topic, timestamp_us, data, fields, .. } = &msg {
            if required.contains(topic.as_str()) {
                for analyzer in &mut analyzers {
                    if analyzer.required_topics().contains(&topic.as_str()) {
                        analyzer.on_message(topic, *timestamp_us, data, fields);
                    }
                }
            }
        }
        SimpleCallbackResult::Continue
    })?;

    // Collect results
    let diagnostics = analyzers
        .into_iter()
        .flat_map(|a| a.finish())
        .collect();

    Ok(DiagnosticsResult { diagnostics })
}
```

### Example: Motor Failure Analyzer

```rust
// crates/converter/src/diagnostics/motor_failure.rs

pub struct MotorFailureAnalyzer {
    /// Track per-motor PWM output over a sliding window.
    motor_windows: Vec<SlidingWindow>,
    detections: Vec<Diagnostic>,
    armed: bool,
}

impl Analyzer for MotorFailureAnalyzer {
    fn required_topics(&self) -> &[&str] {
        &["actuator_outputs", "vehicle_status"]
    }

    fn on_message(&mut self, topic: &str, timestamp_us: u64, data: &[u8], fields: &[FlattenedField]) {
        match topic {
            "vehicle_status" => {
                // Track armed state — only flag failures while armed
                self.armed = read_field_u8(data, fields, "arming_state") == Some(2);
            }
            "actuator_outputs" => {
                if !self.armed { return; }
                // For each motor output, check for:
                // - Sudden drop to 0 PWM (disconnect)
                // - Lock at maximum PWM (saturation for > N ms)
                for (i, window) in self.motor_windows.iter_mut().enumerate() {
                    if let Some(pwm) = read_field_f32(data, fields, &format!("output[{i}]")) {
                        window.push(timestamp_us, pwm);
                        if let Some(failure) = window.detect_failure(timestamp_us) {
                            self.detections.push(failure);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn finish(self) -> Vec<Diagnostic> {
        self.detections
    }
}
```

## Storage & API

### Database

New junction table following the existing pattern (`log_errors`, `log_field_stats`):

```sql
CREATE TABLE IF NOT EXISTS log_diagnostics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    log_id TEXT NOT NULL,
    diagnostic_id TEXT NOT NULL,        -- "motor_failure", "gps_interference"
    severity TEXT NOT NULL,             -- "info", "warning", "critical"
    summary TEXT NOT NULL,
    timestamp_us INTEGER,
    end_timestamp_us INTEGER,
    evidence TEXT,                       -- JSON object
    FOREIGN KEY (log_id) REFERENCES logs(id) ON DELETE CASCADE
);
CREATE INDEX idx_log_diagnostics_log ON log_diagnostics(log_id);
CREATE INDEX idx_log_diagnostics_severity ON log_diagnostics(severity);
```

A `diagnostic_flags` column on the `logs` table (comma-separated diagnostic IDs)
for indexed search, following the `localization_sources` pattern.

### API Response

Diagnostics are included in `metadata.json` and returned by `GET /api/logs/{id}`:

```json
{
  "diagnostics": [
    {
      "id": "motor_failure",
      "summary": "Motor 3 output dropped to 0 PWM at 38.5s while armed",
      "severity": "critical",
      "timestamp_us": 38500000,
      "end_timestamp_us": null,
      "evidence": {
        "motor_index": "3",
        "pwm_value": "0",
        "flight_mode": "Position"
      }
    }
  ]
}
```

### Searchable

Users can filter logs by diagnostic presence:

```
GET /api/logs?diagnostic=motor_failure
GET /api/logs?diagnostic_severity=critical
```

## How to Add a New Analyzer

1. Create `crates/converter/src/diagnostics/your_analyzer.rs`
2. Implement the `Analyzer` trait
3. Register it in `run_diagnostics()` in `mod.rs`
4. Add tests against known-bad ULog fixtures in `tests/fixtures/`
5. Open a PR

Each analyzer is self-contained: it declares its topic dependencies, processes
messages, and emits `Diagnostic` values. No changes to the upload handler, storage
layer, or database schema are needed to add a new analyzer.

## What's In Scope for Contributors

### Phase 1: Deterministic Heuristics (good first issues)

| Analyzer | Topics | Detection |
|----------|--------|-----------|
| Motor failure | `actuator_outputs`, `vehicle_status` | PWM drop to 0 or lock at max while armed |
| GPS interference | `vehicle_gps_position` | EPH/EPV spike, satellite count drop, noise floor increase |
| Battery brownout | `battery_status`, `system_power` | Voltage below critical threshold during flight |
| EKF failure | `estimator_status` | Innovation test ratio exceeding bounds |
| RC loss | `input_rc`, `vehicle_status` | RC signal loss during armed flight |

### Phase 2: Statistical (requires design review)

Statistical or ML-based analyzers (e.g., Isolation Forests for vibration anomaly
detection) are welcome but require additional review:

- Must use Rust-native libraries (`linfa`, `smartcore`, or custom implementations)
- Must document false-positive rates against a test corpus
- Must be clearly labeled as "experimental" in their output severity
- Pre-trained models must be vendored as serialized artifacts, not trained at upload time

## What's NOT In Scope

- Python, Pandas, or any non-Rust runtime dependency
- External service calls during the upload path
- ML model training during upload (inference only)
- Changes to the Parquet storage format
- Separate microservices or job queues
