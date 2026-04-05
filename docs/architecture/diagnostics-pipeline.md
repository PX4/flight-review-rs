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

/// Typed evidence for each diagnostic kind.
///
/// Every analyzer returns a specific variant — not a freeform map. This means
/// the frontend can match on the variant and render structured UI for each
/// diagnostic type without guessing at keys. Adding a new analyzer means
/// adding a new variant here; changing an existing variant is a breaking
/// change that requires a version bump and migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Evidence {
    MotorFailure {
        motor_index: u8,
        pwm_value: f32,
        /// "drop_to_zero" or "locked_at_max"
        failure_mode: String,
        flight_mode: String,
    },
    GpsInterference {
        eph_m: f32,
        epv_m: f32,
        num_satellites: u16,
        noise_level: Option<f32>,
    },
    BatteryBrownout {
        voltage_v: f32,
        critical_threshold_v: f32,
        current_a: Option<f32>,
    },
    EkfFailure {
        /// Which innovation failed (e.g. "velocity", "position", "heading")
        innovation: String,
        test_ratio: f32,
        threshold: f32,
    },
    RcLoss {
        last_signal_timestamp_us: u64,
        signal_lost_duration_ms: u64,
    },
}

/// A single detected anomaly with typed evidence.
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
    /// Typed, structured evidence specific to this diagnostic.
    pub evidence: Evidence,
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

The `Evidence` enum is the contract between analyzers and the frontend. It uses
`#[serde(tag = "type")]` so JSON output is self-describing:

```json
{
  "type": "MotorFailure",
  "motor_index": 3,
  "pwm_value": 0.0,
  "failure_mode": "drop_to_zero",
  "flight_mode": "Position"
}
```

Adding a new analyzer means adding a new `Evidence` variant. The compiler
enforces that every field is populated — no missing keys, no typos, no runtime
surprises. Changing an existing variant's fields is a breaking change that
requires bumping `ANALYSIS_VERSION` and reprocessing affected logs.

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

The upload endpoint is synchronous — the client waits for `convert_ulog()` to
finish before getting a response. Diagnostics run inside that path. If they're
slow, the user sees a slow upload, and at scale it ties up server threads.
This is a hard UX constraint, not just a nice-to-have.

### Absolute Time Budget

The total `convert_ulog()` call — parsing, metadata extraction, analysis
(including diagnostics), and Parquet writing — must complete within:

| File size | Max wall-clock time |
|-----------|-------------------|
| < 10 MB   | **500 ms**        |
| 10–100 MB | **2 seconds**     |
| 100–512 MB| **10 seconds**    |

These limits apply to the full pipeline, not just diagnostics. But diagnostics
must not be the reason the budget is blown.

### CI Enforcement

A conversion benchmark (`benches/convert.rs`) using `criterion` runs against
reference ULog fixtures at each size tier. CI blocks PRs that:

- Push any tier beyond its absolute time budget on the CI runner.
- Regress total conversion time by more than **10%** vs the baseline, even if
  still under the absolute limit.

### What This Means for Contributors

- Keep analyzer logic **O(n)** in message count. No quadratic scans, no
  unbounded buffers.
- Sliding windows must have a **fixed max size**.
- If an analyzer needs heavy computation (e.g., FFT for vibration frequency
  analysis), document the expected overhead and include `cargo bench` results
  in the PR description.
- If your analyzer can't meet the budget, it doesn't ship — optimize first or
  propose making it opt-in / CLI-only.

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
        "type": "MotorFailure",
        "motor_index": 3,
        "pwm_value": 0.0,
        "failure_mode": "drop_to_zero",
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

## Security: Analyzers Process Untrusted Input

Analyzers ship as reviewed Rust code — they are not runtime plugins. But the
data they process is untrusted: anyone can upload a ULog file to the server, and
that file may be crafted to exploit analyzer logic. Since analyzers run inside
the server process (in `spawn_blocking`), a vulnerability here means access to
S3 credentials, RDS connections, and memory shared with other requests.

### Threat Model

- **Malformed ULog fields.** Crafted messages with out-of-bounds offsets,
  unexpected field types, or extreme values designed to trigger panics,
  overflows, or unbounded allocations.
- **Resource exhaustion.** A ULog file with millions of messages for a
  subscribed topic, designed to blow up a sliding window buffer or cause
  O(n²) behavior in an analyzer.
- **Logical manipulation.** Valid-looking data tuned to produce misleading
  diagnostics (false positives at scale could erode trust in the system).

### Required Defenses

Every analyzer PR must satisfy these before merge:

- **Bounds-checked field reads.** All field access must go through safe helper
  functions that validate offsets and sizes before reading. No raw pointer
  arithmetic, no unchecked slice indexing. The existing `RunningStats::update()`
  in `analysis.rs` is the model — it checks `off + N > data.len()` before every
  read.
- **Fixed-size buffers.** Sliding windows and accumulators must have a compile-
  time or config-time maximum size. No `Vec` that grows proportionally to
  message count without a cap.
- **No panics on bad data.** Analyzers must handle malformed input gracefully —
  skip the message, not crash the server. `#[cfg(test)]` fuzz tests against
  randomized/truncated payloads are encouraged.
- **No allocations proportional to untrusted input.** Don't use a user-
  controlled field value as a Vec capacity or HashMap key count.

### CI Enforcement

- A **fuzz test target** (`fuzz/fuzz_diagnostics.rs`) feeds randomized ULog
  byte streams through the full analyzer pipeline. This runs in CI on every PR
  that touches `crates/converter/src/diagnostics/`.
- `#[should_panic]` tests are not allowed in analyzer code — if something can
  panic, it's a bug.

## Output Schema Stability

The diagnostics JSON is a contract between the converter crate and the frontend.
If it changes silently, the frontend breaks. If every analyzer invents its own
shape, the frontend can't render structured UI.

### Typed Evidence Enum

The `Evidence` enum (defined above) is the enforcement mechanism. Each analyzer
returns a specific variant with typed fields — not a freeform `HashMap<String,
String>`. The compiler ensures every field is present and correctly typed at
build time. The frontend matches on `evidence.type` and renders per-diagnostic
UI.

### Snapshot Tests

Every analyzer must include a **snapshot test** that captures the exact JSON
output for its test fixtures. These live alongside the analyzer code:

```
tests/snapshots/
├── motor_failure__crash_log.json.snap
├── motor_failure__normal_flight.json.snap
├── gps_interference__jamming.json.snap
└── ...
```

Using [`insta`](https://docs.rs/insta) for snapshot testing:

```rust
#[test]
fn test_motor_failure_crash_log() {
    let analysis = analyze("tests/fixtures/motor_crash.ulg", &metadata).unwrap();
    let diagnostics: Vec<_> = analysis.diagnostics.iter()
        .filter(|d| d.id == "motor_failure")
        .collect();
    insta::assert_json_snapshot!(diagnostics);
}
```

If an analyzer changes its output — different field values, added fields,
changed structure — the snapshot test fails and the diff is visible in the PR.
Reviewers can see exactly what changed and whether the frontend needs updating.

### Schema Evolution Rules

- **Adding a new `Evidence` variant** (new analyzer): non-breaking. The
  frontend can ignore unknown types gracefully.
- **Adding an `Option<T>` field to an existing variant**: non-breaking.
  Existing JSON deserializes with `None`. Must bump `ANALYSIS_VERSION` so
  backfill populates the new field.
- **Renaming or removing a field from an existing variant**: **breaking**.
  Requires a coordinated frontend + backend change, version bump, and full
  backfill.
- **Changing a field type** (e.g., `f32` → `f64`): **breaking**. Same process.

## How to Add a New Analyzer

1. Create `crates/converter/src/diagnostics/your_analyzer.rs`
2. Add a new variant to the `Evidence` enum for your diagnostic type
3. Implement the `Analyzer` trait
4. Register it in the analyzer list inside `analyze()`
5. Add test fixtures: at least one known-good and one known-bad ULog in
   `tests/fixtures/`
6. Add snapshot tests using `insta` for each fixture
7. Add a fuzz test case covering your analyzer's field parsing
8. Run `cargo bench` and include the results in your PR description
9. Test via the CLI first: `cargo run --bin ulog_convert -- bad_log.ulg -o ./out`
10. Open a PR

Each analyzer is self-contained: it declares its topic dependencies, processes
messages, and emits `Diagnostic` values with typed `Evidence`. No changes to the
upload handler, storage layer, or database schema are needed to add a new
analyzer.

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

## Backfill: Reprocessing Historical Logs

Flight Review serves two audiences: self-hosted users with hundreds of logs and
logs.px4.io with hundreds of thousands. When a new analyzer lands, all
historical logs need reprocessing — but the approach differs by scale.

### Analysis Versioning

Every log record carries an `analysis_version` integer (stored in the `logs`
table). Each time the set of analyzers changes — new analyzer added, existing
one updated — the version constant in the converter crate is bumped. This tells
any reprocessing tool exactly which logs are stale.

```rust
// crates/converter/src/diagnostics/mod.rs
pub const ANALYSIS_VERSION: u32 = 1;
```

### Self-hosted / CLI Users

For small instances, the CLI handles backfill directly:

```
$ cargo run --bin ulog_convert reanalyze \
    --storage s3://my-bucket/logs \
    --db postgres://localhost/flight_review \
    --concurrency 4
```

This iterates logs where `analysis_version < CURRENT`, fetches each `.ulg` from
storage, re-runs `analyze()`, writes updated `metadata.json` back to storage,
updates the DB record and `log_diagnostics` rows, and sets the new version.
Resumable — it picks up where it left off on restart.

### logs.px4.io (Production Scale)

At production scale, reprocessing must not compete with live uploads for CPU,
I/O, or database connections. The backfill runs on **separate compute**:

```
┌──────────────────────┐       ┌──────────────────────┐
│   Serving Instance   │       │   Backfill Worker     │
│                      │       │                       │
│  POST /api/upload    │       │  Triggered by:        │
│  GET  /api/logs      │       │  - deploy to main     │
│                      │       │  - manual invocation   │
│  Writes new logs to  │       │                       │
│  S3 + RDS            │       │  Reads .ulg from S3   │
│                      │       │  Re-runs analyze()    │
│                      │       │  Writes metadata.json │
│                      │       │  Updates RDS          │
└──────┬───────────────┘       └──────┬────────────────┘
       │                              │
       │        ┌─────────┐           │
       └───────►│   S3    │◄──────────┘
       │        └─────────┘           │
       │        ┌─────────┐           │
       └───────►│   RDS   │◄──────────┘
                └─────────┘
```

**Trigger:** A push to `main` that touches `crates/converter/src/diagnostics/`
or bumps `ANALYSIS_VERSION`. This can be an EventBridge rule, a GitHub Actions
workflow that launches an EC2 spot instance, or a simple cron job that checks
the version.

**The worker binary** is the same converter crate compiled as a batch tool — no
new code, just a different entry point:

```
$ flight-review-worker reanalyze \
    --storage s3://logs-px4-io/files \
    --db postgres://rds-endpoint/flight_review \
    --concurrency 8 \
    --batch-size 100 \
    --throttle-ms 50
```

Key properties:

- **Resumable.** Queries `WHERE analysis_version < CURRENT ORDER BY created_at`
  and processes in batches. If it crashes, the next run continues from where it
  stopped — logs already at the current version are skipped.
- **Throttled.** Configurable concurrency and inter-batch delay so it doesn't
  saturate RDS connections or S3 request rates.
- **Isolated.** Runs on its own EC2 instance (spot is fine — resumability makes
  interruptions cheap). Zero impact on serving latency.
- **Observable.** Logs progress to stdout/CloudWatch: total logs, processed,
  remaining, errors, elapsed time.

### What the Serving Instance Does NOT Do

The serving instance never re-analyzes on read. `GET /api/logs/{id}` returns
whatever `metadata.json` is in S3, even if it was generated with an older
analysis version. The backfill worker is the only thing that updates historical
logs. This keeps the read path fast and predictable.

## What's NOT In Scope

- Python, Pandas, or any non-Rust runtime dependency
- External service calls during the upload path
- ML model training during upload (inference only)
- Changes to the Parquet storage format
