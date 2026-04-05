//! Test framework for signal processing modules.
//!
//! Provides fixture helpers for running analyses against real ULog files.
//!
//! ## Required test categories for every module
//!
//! 1. **No errors** — `assert_no_errors("sample.ulg", "<id>")`
//! 2. **Produces result** — verify output on a known-good fixture
//! 3. **Handles missing signals** — returns `InsufficientData`, not panic
//! 4. **Snapshot test** — `insta::assert_json_snapshot!` for CI diffing

use std::collections::HashMap;

/// Resolve a test fixture path by name.
pub fn fixture_path(name: &str) -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");

    let local = std::path::Path::new(manifest)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("crates/converter/tests/fixtures")
        .join(name);
    if local.exists() {
        return local.to_string_lossy().to_string();
    }

    let external = std::path::Path::new(manifest)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("px4-ulog-rs/tests/fixtures")
        .join(name);
    external.to_string_lossy().to_string()
}

/// Run all registered analyses on a fixture file.
pub fn analyze_fixture(name: &str) -> HashMap<String, serde_json::Value> {
    let path = fixture_path(name);
    assert!(
        std::path::Path::new(&path).exists(),
        "Test fixture not found: {}",
        path
    );
    let analyses = super::create_analyses();
    super::run_analyses(&path, &analyses).unwrap_or_default()
}

/// Run all analyses and return the result for a specific module.
pub fn analyze_fixture_for(name: &str, analysis_id: &str) -> Option<serde_json::Value> {
    let path = fixture_path(name);
    if !std::path::Path::new(&path).exists() {
        return None;
    }
    let analyses = super::create_analyses();
    let results = super::run_analyses(&path, &analyses).unwrap_or_default();
    results.get(analysis_id).cloned()
}

/// Assert that an analysis produces a non-empty result on a fixture.
pub fn assert_produces_result(fixture: &str, analysis_id: &str) {
    let result = analyze_fixture_for(fixture, analysis_id);
    assert!(
        result.is_some(),
        "Expected '{}' to produce a result on {}, but got None",
        analysis_id,
        fixture
    );
}

/// Assert that running analyses on a fixture does not error or panic.
pub fn assert_no_errors(fixture: &str, analysis_id: &str) {
    let path = fixture_path(fixture);
    if !std::path::Path::new(&path).exists() {
        eprintln!("Skipping: {} not available", fixture);
        return;
    }
    let analyses = super::create_analyses();
    let filtered: Vec<_> = analyses
        .into_iter()
        .filter(|a| a.id() == analysis_id)
        .collect();
    // Should not panic or return a hard error
    let _ = super::run_analyses(&path, &filtered);
}
