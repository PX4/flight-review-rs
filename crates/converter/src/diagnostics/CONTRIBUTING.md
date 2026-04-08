# Adding a diagnostic analyzer

This guide walks through adding a new analyzer to the diagnostics pipeline. The pipeline is defined in [`mod.rs`](./mod.rs) and runs during the streaming `analyze()` pass: each analyzer declares the ULog topics it needs, receives messages one at a time, and emits structured `Diagnostic` results at the end.

If you're new to the module, read [`rc_loss.rs`](./rc_loss.rs) first — it's the shortest complete reference implementation.

![Contributor workflow](./docs/workflow.svg)

## What the pipeline looks like

![Diagnostics pipeline](./docs/pipeline.svg)

Key properties to keep in mind when designing an analyzer:

- **Single-pass, streaming.** You see each message exactly once, in timestamp order. You don't get a random-access view of the whole log. If your detection needs a window, buffer it yourself inside the analyzer struct.
- **One log at a time.** There is no batch/training phase. If your detector needs prior data (e.g. a trained model), it has to be baked into the binary as a constant, loaded from disk at startup, or derived from the current log's early samples before you start emitting results.
- **Topic-scoped dispatch.** Messages are only delivered for topics you list in `required_topics()`. Don't try to filter them yourself in `on_message` — just declare what you need.
- **Performance budget.** The whole diagnostic pass has a 500ms budget enforced by `cargo bench` in CI. A ~4MB log currently runs in ~37ms end-to-end. Stay cheap per message.

## Step 1: Add an `Evidence` variant

Every analyzer emits typed, structured evidence — not freeform strings or maps. The frontend matches on `evidence.type` and renders a specific UI for each variant. Changing an existing variant's fields is a breaking schema change, so pick field names carefully the first time.

Edit [`mod.rs`](./mod.rs) and add your variant:

```rust
pub enum Evidence {
    // ... existing variants ...
    ZAxisVibrationAnomaly {
        score: f32,
        peak_accel_m_s2: f32,
        window_start_us: u64,
        window_end_us: u64,
    },
}
```

Also bump `ANALYSIS_VERSION` in the same file when your analyzer is ready to ship. That tells the reprocessing pipeline historical logs need a re-scan.

## Step 2: Create the analyzer file

Create `crates/converter/src/diagnostics/your_analyzer.rs`. Minimal skeleton:

```rust
//! Short description of what this analyzer detects and how.
//!
//! Topics consumed, thresholds used, and any known limitations or fixture gaps
//! (use SKIP_FIXTURE: <reason> if no real-world log exists yet).

use super::{parse_field, Analyzer, Diagnostic, Evidence, Severity};
use px4_ulog::stream_parser::model::DataMessage;

const SOME_THRESHOLD: f32 = 2.5;

pub struct YourAnalyzer {
    // Per-flight state. Reset per log.
    detections: Vec<Diagnostic>,
}

impl Default for YourAnalyzer {
    fn default() -> Self { Self::new() }
}

impl YourAnalyzer {
    pub fn new() -> Self {
        Self { detections: Vec::new() }
    }
}

impl Analyzer for YourAnalyzer {
    fn id(&self) -> &str { "your_analyzer" }

    fn description(&self) -> &str { "One-line human description" }

    fn required_topics(&self) -> &[&str] {
        &["sensor_combined", "vehicle_status"]
    }

    fn on_message(&mut self, data: &DataMessage) {
        let topic = data.flattened_format.message_name.as_str();
        let ts = data
            .flattened_format
            .timestamp_field
            .as_ref()
            .map(|tf| tf.parse_timestamp(data.data))
            .unwrap_or(0);

        match topic {
            "sensor_combined" => {
                let Some(az) = parse_field::<f32>(data, "accelerometer_m_s2[2]") else {
                    return;
                };
                // ... update state, maybe push to self.detections ...
                let _ = (ts, az);
            }
            _ => {}
        }
    }

    fn finish(self: Box<Self>) -> Vec<Diagnostic> {
        self.detections
    }
}
```

Notes on the trait methods:

- **`id()`** is the stable machine identifier stored in the database and exposed via the API's `?diagnostic=` filter. Don't change it after release.
- **`required_topics()`** must match the exact ULog topic names. Typos mean your analyzer silently never runs.
- **`on_message()`** must not panic and must handle missing fields gracefully — use `parse_field::<T>()`, which returns `Option<T>`, never unwrap.
- **`finish()`** takes `Box<Self>` (the pipeline owns the analyzers). Move your accumulated detections out and return them.

## Step 3: Register it

In [`mod.rs`](./mod.rs), add your analyzer to `create_analyzers()`:

```rust
pub fn create_analyzers() -> Vec<Box<dyn Analyzer>> {
    vec![
        // ... existing ones ...
        Box::new(your_analyzer::YourAnalyzer::new()),
    ]
}
```

And add the `pub mod your_analyzer;` declaration at the top of the file.

Until you do this, nothing in the pipeline will ever construct or call your analyzer. This is the step most first-time contributors miss.

## Step 4: Write the required tests

CI runs [`scripts/ci/check-analyzer.sh`](../../../../scripts/ci/check-analyzer.sh) on every PR touching this directory. It grep-checks your file for a specific test pattern. At minimum you need:

1. **`no_false_positives_sample`** — runs your analyzer against `tests/fixtures/sample.ulg` (a normal flight) and asserts zero detections.
2. **A real-world detection test** named `detects_real_*` — points at a fixture ULog that actually exhibits the anomaly, asserts the detection fires with the right severity/evidence. If no fixture exists, add `SKIP_FIXTURE: <reason>` to the module doc comment and open an issue to collect one.
3. **`handles_missing_fields`** — feed it a message with no fields and assert it doesn't panic and emits nothing.
4. **At least one synthetic detection test** — uses `MessageBuilder` from [`testing.rs`](./testing.rs) to construct messages by hand. This is where you pin down your detection logic with fast deterministic tests.
5. **A snapshot test** using `insta::assert_json_snapshot!` on the fixture output. Run `cargo insta review` locally to accept the first snapshot.

Copy the test block from [`rc_loss.rs`](./rc_loss.rs) and adapt it — it hits every required category.

## Step 5: Run the same gates CI will

Before opening a PR, run locally:

```sh
# The trait/test/registration checker CI uses
scripts/ci/check-analyzer.sh

# The diagnostic test suite
cargo test -p flight-review --lib diagnostics

# The performance budget
cargo bench -p flight-review --bench convert
```

If `check-analyzer.sh` complains, it will tell you exactly which of the five criteria you missed. If the bench regresses past the budget, profile your `on_message` — the usual culprit is allocating or parsing the same field multiple times per message.

## Common first-time mistakes

- **Defining a new `Analyzer` trait.** There's already one in [`mod.rs`](./mod.rs). Implement it; don't redefine it.
- **Putting the file outside `diagnostics/`.** It has to live in this directory, otherwise the CI checker and the registration factory won't see it.
- **Returning `Option<String>` or a freeform summary.** Results must be `Vec<Diagnostic>` with a typed `Evidence` variant.
- **Assuming you get the whole log at once.** You don't. Design for streaming.
- **Pulling in heavy ML dependencies without discussing the perf/memory budget first.** The converter is zero-ML today; open an issue before adding something like `extended-isolation-forest`, `smartcore`, etc. so we can agree on how the model is trained, shipped, and benchmarked.
- **Skipping the real-world fixture.** Synthetic tests alone don't count toward the CI gate. Either ship a fixture or mark `SKIP_FIXTURE` with a reason.

## Questions

Open a draft PR early and tag `@mrpollo`. Draft PRs are the right place to get architecture feedback before you go deep on implementation.
