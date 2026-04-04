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
5. **CLI-first development.** All analyzers must work via the `ulog_convert`
   CLI tool first. The server calls the same code — it is not the place to
   develop or validate new analysis.
6. **Performance-budgeted.** Every analyzer runs within a measured time budget.
   PRs that regress conversion benchmarks beyond the allowed threshold are
   blocked by CI.

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

Diagnostics are **not** a separate pass. They run inside the existing
`analyze()` streaming callback, alongside the current stats, battery, GPS, and
vibration collectors. This avoids adding a redundant pass over the same data:

```
convert_ulog()
├── extract_metadata()                  // 1st streaming pass
├── analyze()                           // 2nd streaming pass
│   ├── existing collectors (stats, battery, GPS, vibration, modes, ...)
│   └── diagnostic analyzers            // NEW — same pass, same callback
└── write Parquet files
```

Each analyzer registers the topics it cares about. The `analyze()` callback
dispatches each data message to the relevant collectors *and* the relevant
analyzers in the same loop iteration. One pass, no waste.

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
    /// The analyze() callback will only dispatch messages for these topics.
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

### Integration into analyze()

The existing `analyze()` function builds its collectors (flight modes, stats,
battery, GPS, vibration, field stats) and runs a single streaming pass.
Diagnostic analyzers plug into this same pass:

```rust
pub fn analyze(
    path: &str,
    metadata: &FlightMetadata,
) -> Result<FlightAnalysis, std::io::Error> {
    // ... existing collector setup ...

    // Build diagnostic analyzers
    let mut analyzers: Vec<Box<dyn Analyzer>> = vec![
        Box::new(MotorFailureAnalyzer::new(metadata)),
        Box::new(GpsInterferenceAnalyzer::new(metadata)),
        // Contributors add new analyzers here
    ];

    let diagnostic_topics: HashSet<&str> = analyzers
        .iter()
        .flat_map(|a| a.required_topics())
        .collect();

    read_file_with_simple_callback(path, |msg| {
        if let Message::Data { topic, timestamp_us, data, fields, .. } = &msg {
            // ... existing collector dispatch (unchanged) ...

            // Dispatch to diagnostic analyzers
            if diagnostic_topics.contains(topic.as_str()) {
                for analyzer in &mut analyzers {
                    if analyzer.required_topics().contains(&topic.as_str()) {
                        analyzer.on_message(topic, *timestamp_us, data, fields);
                    }
                }
            }
        }
        SimpleCallbackResult::Continue
    })?;

    // Collect diagnostics alongside existing results
    let diagnostics: Vec<Diagnostic> = analyzers
        .into_iter()
        .flat_map(|a| a.finish())
        .collect();

    Ok(FlightAnalysis {
        // ... existing fields ...
        diagnostics,
    })
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

## Implementation Order: CLI First, Server Second

All diagnostic work follows this sequence:

1. **CLI (`ulog_convert`)** — Implement and validate the analyzer against local
   ULog fixtures. The CLI is the development and testing surface. Run it against
   known-bad logs, inspect the output, iterate.

2. **Tests** — Unit tests against fixture files in `tests/fixtures/`. Each
   analyzer must include at least one known-good log (no false positives) and
   one known-bad log (detection fires correctly).

3. **Server** — The server calls the same `analyze()` function. Once the CLI
   output is correct the server gets it for free. No server-specific diagnostic
   code.

```
# Development workflow
$ cargo run --bin ulog_convert -- input.ulg --output-dir ./out
# inspect ./out/metadata.json → diagnostics array

# NOT this:
$ curl -F file=@input.ulg http://localhost:8080/api/upload
# ^ this is for validation, not development
```

## Performance Budget

Every analyzer adds work to the `analyze()` streaming pass. To prevent
regressions:

### Benchmark Gate

A conversion benchmark (`benches/convert.rs`) measures end-to-end
`convert_ulog()` time against a reference ULog fixture. CI enforces:

- **Threshold:** PRs that regress conversion time by more than **10%** vs the
  baseline are blocked.
- **Measurement:** `cargo bench` using `criterion`, run on CI with a pinned
  fixture file.
- **Per-analyzer overhead:** Each analyzer should add no more than **5%** to
  the total `analyze()` pass time. If it does, it needs optimization or must
  justify the cost in the PR description.

### What This Means for Contributors

- Keep analyzer logic O(n) in message count. No quadratic scans, no
  unbounded buffers.
- Sliding windows should have a fixed max size.
- If an analyzer needs heavy computation (e.g., FFT for vibration frequency
  analysis), document the expected overhead and benchmark it.

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
3. Register it in the analyzer list inside `analyze()`
4. Add test fixtures: at least one known-good and one known-bad ULog in
   `tests/fixtures/`
5. Run `cargo bench` and include the results in your PR description
6. Test via the CLI first: `cargo run --bin ulog_convert -- bad_log.ulg -o ./out`
7. Open a PR

Each analyzer is self-contained: it declares its topic dependencies, processes
messages, and emits `Diagnostic` values. No changes to the upload handler,
storage layer, or database schema are needed to add a new analyzer.

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
- Pre-trained models must be vendored as serialized artifacts, not trained at
  upload time

#### Pre-trained Model Licensing Requirements

Any vendored model artifact (serialized weights, decision trees, etc.) must meet
all of the following before it can be merged:

- **License compatibility:** The model and its artifacts must be released under
  a license compatible with this project (Apache-2.0 or MIT). No GPL, no
  "non-commercial", no "research only" restrictions.
- **Training data provenance:** The PR must document what data was used to train
  the model, where that data came from, and under what license that data was
  collected. Models trained on proprietary or non-redistributable flight logs
  will not be accepted.
- **Reproducibility:** The PR must include or reference scripts that can
  regenerate the model artifact from publicly available data. "Trust me, I
  trained it" is not sufficient.
- **Size budget:** Model artifacts must be under **1 MB** uncompressed. Larger
  models need explicit maintainer approval and a justification for the size.

## What's NOT In Scope

- Python, Pandas, or any non-Rust runtime dependency
- External service calls during the upload path
- ML model training during upload (inference only)
- Changes to the Parquet storage format
- Separate microservices or job queues
