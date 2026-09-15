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

/// Run all registered analyses on a fixture file.
pub fn analyze_fixture(name: &str) -> HashMap<String, serde_json::Value> {
    let path = fixture_path(name);
    assert!(
        std::path::Path::new(&path).exists(),
        "Test fixture not found: {}",
        path
    );
    let analyses = super::create_analyses();
    super::run_analyses(&path, &analyses).expect("signal analysis failed")
}

/// Run all analyses and return the result for a specific module.
pub fn analyze_fixture_for(name: &str, analysis_id: &str) -> Option<serde_json::Value> {
    assert!(
        super::create_analyses()
            .iter()
            .any(|a| a.id() == analysis_id),
        "unknown signal analysis: {analysis_id}"
    );
    analyze_fixture(name).remove(analysis_id)
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
    assert!(
        std::path::Path::new(&path).exists(),
        "Test fixture not found: {path}"
    );
    let analyses = super::create_analyses();
    let filtered: Vec<_> = analyses
        .into_iter()
        .filter(|a| a.id() == analysis_id)
        .collect();
    assert!(
        !filtered.is_empty(),
        "unknown signal analysis: {analysis_id}"
    );
    super::run_analyses(&path, &filtered).expect("signal analysis failed");
}

/// Deterministic broadband input, independent of external fixtures and RNG versions.
pub fn broadband(samples: usize) -> Vec<f64> {
    let mut state = 0x1234_5678_u32;
    (0..samples)
        .map(|_| {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            2.0 * state as f64 / u32::MAX as f64 - 1.0
        })
        .collect()
}

/// Minimal real ULog framing with current PX4 rate setpoint and gyro schemas.
/// Kept under the project target directory, never the system temporary directory.
pub struct PidLog {
    _directory: tempfile::TempDir,
    pub path: std::path::PathBuf,
}

impl PidLog {
    pub fn new(
        setpoint: &[(u64, [f32; 3])],
        gyro: &[(u64, [f32; 3])],
        other_gyro: &[(u64, [f32; 3])],
    ) -> Self {
        fn message(bytes: &mut Vec<u8>, kind: u8, payload: &[u8]) {
            bytes.extend_from_slice(&(payload.len() as u16).to_le_bytes());
            bytes.push(kind);
            bytes.extend_from_slice(payload);
        }
        let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
        std::fs::create_dir_all(&target).unwrap();
        let directory = tempfile::tempdir_in(target).unwrap();
        let path = directory.path().join("pid.ulg");
        let mut bytes = b"ULog\x01\x12\x35\x01".to_vec();
        bytes.extend_from_slice(&1_000_000_u64.to_le_bytes());
        message(&mut bytes, b'B', &[0; 40]);
        message(
            &mut bytes,
            b'F',
            b"vehicle_rates_setpoint:uint64_t timestamp;float roll;float pitch;float yaw;float[3] thrust_body;bool reset_integral;",
        );
        message(
            &mut bytes,
            b'F',
            b"vehicle_angular_velocity:uint64_t timestamp;uint64_t timestamp_sample;float[3] xyz;",
        );
        for (id, instance, name) in [
            (1_u16, 0, "vehicle_rates_setpoint"),
            (2, 0, "vehicle_angular_velocity"),
            (3, 1, "vehicle_angular_velocity"),
        ] {
            let mut payload = vec![instance];
            payload.extend_from_slice(&id.to_le_bytes());
            payload.extend_from_slice(name.as_bytes());
            message(&mut bytes, b'A', &payload);
        }
        // Interleave topics as a flight logger does, preserving each source's
        // supplied arrival order (including deliberately invalid timestamps).
        for index in 0..setpoint.len().max(gyro.len()).max(other_gyro.len()) {
            for (id, stream) in [(1_u16, setpoint), (2, gyro), (3, other_gyro)] {
                let Some((timestamp, values)) = stream.get(index) else {
                    continue;
                };
                let mut payload = id.to_le_bytes().to_vec();
                payload.extend_from_slice(&timestamp.to_le_bytes());
                if id != 1 {
                    payload.extend_from_slice(&timestamp.saturating_sub(200).to_le_bytes());
                }
                for value in values {
                    payload.extend_from_slice(&value.to_le_bytes());
                }
                if id == 1 {
                    for value in [0.0_f32, 0.0, -0.5] {
                        payload.extend_from_slice(&value.to_le_bytes());
                    }
                    payload.push(0);
                }
                message(&mut bytes, b'D', &payload);
            }
        }
        std::fs::write(&path, bytes).unwrap();
        Self {
            _directory: directory,
            path,
        }
    }

    pub fn analyze(&self) -> HashMap<String, serde_json::Value> {
        super::run_analyses(self.path.to_str().unwrap(), &super::create_analyses())
            .expect("synthetic ULog analysis failed")
    }
}

#[test]
#[should_panic(expected = "Test fixture not found")]
fn missing_fixtures_fail() {
    analyze_fixture_for("nonexistent-signal-fixture.ulg", "pid_step_response");
}

#[test]
#[should_panic(expected = "Test fixture not found")]
fn missing_no_errors_fixture_fails() {
    assert_no_errors("nonexistent-signal-fixture.ulg", "pid_step_response");
}

#[test]
fn fixture_helpers_do_not_swallow_parser_errors() {
    let log = PidLog::new(&[], &[], &[]);
    std::fs::write(&log.path, b"not a valid ULog header").unwrap();
    let path = log.path.to_str().unwrap();
    assert!(std::panic::catch_unwind(|| analyze_fixture(path)).is_err());
    assert!(std::panic::catch_unwind(|| analyze_fixture_for(path, "pid_step_response")).is_err());
    assert!(std::panic::catch_unwind(|| assert_no_errors(path, "pid_step_response")).is_err());
}
