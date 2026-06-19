//! Test framework for diagnostic analyzers.
//!
//! Provides two approaches for testing analyzers:
//!
//! ## Approach A: Synthetic messages with [`MessageBuilder`]
//!
//! Build `FlattenedFormat` + raw byte buffers to unit-test analyzers in
//! isolation without real ULog files:
//!
//! ```rust,ignore
//! let (fmt, data) = MessageBuilder::new("actuator_outputs")
//!     .timestamp(1_000_000)
//!     .field_f32("output[0]", 1500.0)
//!     .field_f32("output[1]", 0.0)
//!     .build();
//! let dm = make_data_message(&fmt, &data);
//! analyzer.on_message(&dm);
//! ```
//!
//! ## Approach B: Full pipeline on fixture files
//!
//! ```rust,ignore
//! assert_no_false_positives("sample.ulg", "motor_failure");
//! ```
//!
//! ## Required test categories for every analyzer
//!
//! 1. **No false positives** — `assert_no_false_positives("sample.ulg", "<id>")`
//! 2. **Detection** — synthetic bad data via `MessageBuilder` for each failure mode
//! 3. **Missing fields** — messages with missing fields must not panic
//! 4. **Deduplication** — same failure doesn't fire repeatedly
//! 5. **Snapshot** — `insta::assert_json_snapshot!` for CI diffing
//! 6. **Real-world fixture** — a `.ulg` file in `tests/fixtures/` that exhibits
//!    the failure mode, with a test that asserts detection fires. Name it after
//!    the analyzer (e.g. `motor_failure.ulg`). Use `ulog-convert scan` against
//!    a corpus to find candidate files. Without a real fixture, the analyzer
//!    is untested against actual PX4 telemetry and will not be accepted.

use super::Diagnostic;
use px4_ulog::stream_parser::model::{
    DataMessage, FlattenedField, FlattenedFieldType, FlattenedFormat, MultiId,
};

/// Builder for constructing synthetic DataMessage values in tests.
///
/// Handles byte layout automatically — tracks field offsets, writes
/// little-endian values, and builds a valid `FlattenedFormat`.
pub struct MessageBuilder {
    topic: String,
    fields: Vec<FlattenedField>,
    data: Vec<u8>,
    offset: u16,
    has_timestamp: bool,
}

impl MessageBuilder {
    /// Start building a message for the given topic.
    pub fn new(topic: &str) -> Self {
        Self {
            topic: topic.to_string(),
            fields: Vec::new(),
            data: Vec::new(),
            offset: 0,
            has_timestamp: false,
        }
    }

    /// Write a u64 timestamp at the current offset.
    /// This should typically be the first field added.
    pub fn timestamp(mut self, value: u64) -> Self {
        self.fields.push(FlattenedField {
            flattened_field_name: "timestamp".to_string(),
            field_type: FlattenedFieldType::UInt64,
            offset: self.offset,
        });
        self.data.extend_from_slice(&value.to_le_bytes());
        self.offset += 8;
        self.has_timestamp = true;
        self
    }

    /// Append a f32 field.
    pub fn field_f32(mut self, name: &str, value: f32) -> Self {
        self.fields.push(FlattenedField {
            flattened_field_name: name.to_string(),
            field_type: FlattenedFieldType::Float,
            offset: self.offset,
        });
        self.data.extend_from_slice(&value.to_le_bytes());
        self.offset += 4;
        self
    }

    /// Append a f64 field.
    pub fn field_f64(mut self, name: &str, value: f64) -> Self {
        self.fields.push(FlattenedField {
            flattened_field_name: name.to_string(),
            field_type: FlattenedFieldType::Double,
            offset: self.offset,
        });
        self.data.extend_from_slice(&value.to_le_bytes());
        self.offset += 8;
        self
    }

    /// Append a u8 field.
    pub fn field_u8(mut self, name: &str, value: u8) -> Self {
        self.fields.push(FlattenedField {
            flattened_field_name: name.to_string(),
            field_type: FlattenedFieldType::UInt8,
            offset: self.offset,
        });
        self.data.push(value);
        self.offset += 1;
        self
    }

    /// Append a u16 field.
    pub fn field_u16(mut self, name: &str, value: u16) -> Self {
        self.fields.push(FlattenedField {
            flattened_field_name: name.to_string(),
            field_type: FlattenedFieldType::UInt16,
            offset: self.offset,
        });
        self.data.extend_from_slice(&value.to_le_bytes());
        self.offset += 2;
        self
    }

    /// Append a u32 field.
    pub fn field_u32(mut self, name: &str, value: u32) -> Self {
        self.fields.push(FlattenedField {
            flattened_field_name: name.to_string(),
            field_type: FlattenedFieldType::UInt32,
            offset: self.offset,
        });
        self.data.extend_from_slice(&value.to_le_bytes());
        self.offset += 4;
        self
    }

    /// Append an i32 field.
    pub fn field_i32(mut self, name: &str, value: i32) -> Self {
        self.fields.push(FlattenedField {
            flattened_field_name: name.to_string(),
            field_type: FlattenedFieldType::Int32,
            offset: self.offset,
        });
        self.data.extend_from_slice(&value.to_le_bytes());
        self.offset += 4;
        self
    }

    /// Append a u64 field (non-timestamp).
    pub fn field_u64(mut self, name: &str, value: u64) -> Self {
        self.fields.push(FlattenedField {
            flattened_field_name: name.to_string(),
            field_type: FlattenedFieldType::UInt64,
            offset: self.offset,
        });
        self.data.extend_from_slice(&value.to_le_bytes());
        self.offset += 8;
        self
    }

    /// Build the FlattenedFormat and byte buffer.
    /// Returns owned data suitable for use with [`make_data_message`].
    pub fn build(self) -> (FlattenedFormat, Vec<u8>) {
        let format = FlattenedFormat::new(self.topic, self.fields, self.offset)
            .expect("MessageBuilder produced invalid FlattenedFormat");
        (format, self.data)
    }
}

/// Create a `DataMessage` reference from owned format and data.
/// Use after [`MessageBuilder::build()`].
pub fn make_data_message<'a>(format: &'a FlattenedFormat, data: &'a [u8]) -> DataMessage<'a> {
    DataMessage {
        msg_id: 0,
        multi_id: MultiId::new(0),
        flattened_format: format,
        data,
    }
}

/// Resolve a test fixture path by name from the converter crate's fixtures.
pub fn fixture_path(name: &str) -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    std::path::Path::new(manifest)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("crates/converter/tests/fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

/// Run the full analysis pipeline on a ULog fixture and return diagnostics.
/// Panics if the fixture doesn't exist.
pub fn analyze_fixture(name: &str) -> Vec<Diagnostic> {
    let path = fixture_path(name);
    assert!(
        std::path::Path::new(&path).exists(),
        "Test fixture not found: {}",
        path
    );
    let meta = crate::metadata::extract_metadata(&path).unwrap();
    let analysis = crate::analysis::analyze(&path, &meta).unwrap();
    analysis.diagnostics
}

/// Run the full pipeline and filter to a specific analyzer by diagnostic ID.
pub fn analyze_fixture_for(name: &str, diagnostic_id: &str) -> Vec<Diagnostic> {
    analyze_fixture(name)
        .into_iter()
        .filter(|d| d.id == diagnostic_id)
        .collect()
}

/// Assert that an analyzer produces zero diagnostics on a fixture (no false positives).
pub fn assert_no_false_positives(fixture: &str, diagnostic_id: &str) {
    let diags = analyze_fixture_for(fixture, diagnostic_id);
    assert!(
        diags.is_empty(),
        "Expected no '{}' diagnostics on {}, but found {}: {:?}",
        diagnostic_id,
        fixture,
        diags.len(),
        diags
    );
}

/// Assert descriptor field names == evidence JSON keys (minus "type" tag)
/// for every analyzer with a `{id}.ulg` fixture.
#[test]
fn descriptor_fields_match_evidence_keys() {
    use std::collections::BTreeSet;

    let analyzers = super::create_analyzers();
    let mut tested = Vec::new();
    let mut skipped = Vec::new();

    for analyzer in analyzers {
        let id = analyzer.id().to_string();
        let descriptor = analyzer.output_descriptor();

        // Discover fixture by convention
        let fixture_name = format!("{}.ulg", id);
        let fixture_file = fixture_path(&fixture_name);
        if !std::path::Path::new(&fixture_file).exists() {
            skipped.push(id);
            continue;
        }

        let diags = analyze_fixture_for(&fixture_name, &id);
        assert!(
            !diags.is_empty(),
            "analyzer '{}' produced no diagnostics from fixture '{}' — \
             can't verify descriptor parity",
            id, fixture_name,
        );

        let descriptor_names: BTreeSet<String> = descriptor
            .fields
            .iter()
            .map(|f| f.name.clone())
            .collect();

        for diag in &diags {
            let evidence_json = serde_json::to_value(&diag.evidence)
                .expect("Evidence must serialize");
            let evidence_obj = evidence_json
                .as_object()
                .expect("Evidence must serialize to JSON object");

            let evidence_names: BTreeSet<String> = evidence_obj
                .keys()
                .filter(|k| *k != "type")
                .cloned()
                .collect();

            assert_eq!(
                descriptor_names, evidence_names,
                "descriptor/evidence field mismatch for '{}'\n\
                 descriptor declares: {:?}\n\
                 evidence contains:   {:?}\n\
                 missing from descriptor: {:?}\n\
                 extra in descriptor:     {:?}",
                id,
                descriptor_names,
                evidence_names,
                evidence_names.difference(&descriptor_names).collect::<Vec<_>>(),
                descriptor_names.difference(&evidence_names).collect::<Vec<_>>(),
            );
        }

        tested.push(id);
    }

    // Ensure we actually tested something — if all analyzers are skipped,
    // this test is vacuous and should fail loudly.
    assert!(
        !tested.is_empty(),
        "no analyzers were tested — all skipped: {:?}",
        skipped,
    );

    if !skipped.is_empty() {
        eprintln!(
            "NOTE: skipped descriptor parity check for analyzers without fixtures: {:?}",
            skipped,
        );
    }
}
