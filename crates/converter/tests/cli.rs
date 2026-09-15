use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

fn workspace() -> TempDir {
    tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap()
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/motor_failure.ulg")
}

fn copy_log(path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::copy(fixture(), path).unwrap();
}

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flight-review"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn records(output: &Output) -> Vec<Value> {
    serde_json::Deserializer::from_slice(&output.stdout)
        .into_iter::<Value>()
        .map(Result::unwrap)
        .collect()
}

fn validate_export(path: &Path) {
    let manifest: Value =
        serde_json::from_slice(&fs::read(path.join("manifest.json")).unwrap()).unwrap();
    let metadata: Value =
        serde_json::from_slice(&fs::read(path.join("metadata.json")).unwrap()).unwrap();
    assert!(metadata["analysis"].is_object());
    assert!(!manifest["topics"].as_object().unwrap().is_empty());
    for filename in manifest["topics"].as_object().unwrap().values() {
        let file = fs::File::open(path.join(filename.as_str().unwrap())).unwrap();
        let builder =
            parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(file).unwrap();
        let mut reader = builder.build().unwrap();
        assert!(reader.next().unwrap().unwrap().num_rows() > 0);
    }
}

#[test]
fn default_file_matches_explicit_analysis_without_artifacts() {
    let root = workspace();
    copy_log(&root.path().join("flight.ulg"));
    let implicit = run(root.path(), &["flight.ulg"]);
    let explicit = run(root.path(), &["analyze", "flight.ulg"]);
    success(&implicit);
    success(&explicit);
    assert_eq!(records(&implicit), records(&explicit));
    let record = &records(&implicit)[0];
    assert_eq!(record["outcome"], "analyzed");
    assert_eq!(
        record["modules"]["pid_step_response"]["status"],
        "unavailable"
    );
    assert_eq!(
        record["diagnostic_outcomes"].as_object().unwrap().len(),
        flight_review::diagnostics::create_analyzers().len()
    );
    assert!(record["modules"]["pid_step_response"]["message"]
        .as_str()
        .unwrap()
        .contains("No axis met PID step-response criteria"));
    assert!(record["summary"]["message_count"].as_u64().unwrap() > 0);
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    assert_eq!(String::from_utf8_lossy(&implicit.stdout).lines().count(), 1);
    assert_eq!(
        serde_json::from_slice::<Value>(&implicit.stdout).unwrap(),
        *record
    );
    assert!(implicit.stderr.is_empty());
}

#[test]
fn default_json_preserves_finding_evidence_and_locations() {
    let root = workspace();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tecs_nonfinite_pitch.ulg"),
        root.path().join("flight.ulg"),
    )
    .unwrap();
    let output = run(root.path(), &["flight.ulg"]);
    success(&output);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let finding = report["analysis"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|finding| finding["id"] == "tecs_nonfinite_pitch")
        .unwrap();
    assert!(finding["evidence"].is_object());
    assert_eq!(finding["anchor"]["topic"], "tecs_status");
    assert!(finding["anchor"]["field"]
        .as_str()
        .unwrap()
        .starts_with("pitch_"));
    assert!(finding["timestamp_us"].as_u64().unwrap() > 0);
    assert!(
        finding["kind"]["region"]["end_timestamp_us"]
            .as_u64()
            .unwrap()
            > finding["timestamp_us"].as_u64().unwrap()
    );
}

#[test]
fn directory_is_recursive_stable_and_matches_explicit_analysis() {
    let root = workspace();
    for name in ["logs/z/flight.ulg", "logs/a/flight.ULG", "logs/b.ulg"] {
        copy_log(&root.path().join(name));
    }
    fs::write(root.path().join("logs/ignored.txt"), "not a log").unwrap();
    let implicit = run(root.path(), &["logs", "--jobs", "2"]);
    let explicit = run(root.path(), &["analyze", "logs", "--jobs", "1"]);
    success(&implicit);
    success(&explicit);
    assert_eq!(records(&implicit), records(&explicit));
    let records = records(&implicit);
    assert_eq!(
        records
            .iter()
            .map(|r| r["file"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["logs/a/flight.ULG", "logs/b.ulg", "logs/z/flight.ulg"]
    );
    assert_eq!(
        implicit
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .count(),
        3
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn empty_missing_and_corrupt_inputs_fail_with_error_records() {
    let root = workspace();
    fs::create_dir(root.path().join("empty")).unwrap();
    fs::write(root.path().join("corrupt.ulg"), b"not a ULog file").unwrap();
    fs::write(root.path().join("zero.ulg"), b"").unwrap();
    for input in ["empty", "missing.ulg", "corrupt.ulg", "zero.ulg"] {
        let output = run(root.path(), &[input]);
        assert!(!output.status.success(), "{input}");
        let record = &records(&output)[0];
        assert_eq!(record["file"], input);
        assert_eq!(record["outcome"], "error");
        assert!(record["error"].is_string());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
    }
}

#[test]
fn mixed_directory_reports_good_and_bad_and_fails() {
    let root = workspace();
    copy_log(&root.path().join("logs/good.ulg"));
    fs::write(root.path().join("logs/bad.ulg"), b"bad").unwrap();
    let output = run(root.path(), &["logs", "--analyzer", "rc_loss"]);
    assert!(!output.status.success());
    let records = records(&output);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["outcome"], "error");
    assert_eq!(records[1]["outcome"], "analyzed");
    assert_eq!(
        records[1]["diagnostic_outcomes"].as_object().unwrap().len(),
        1
    );
}

#[test]
fn convert_and_analysis_export_write_complete_datasets() {
    let root = workspace();
    copy_log(&root.path().join("flight.ulg"));
    for args in [
        vec!["convert", "flight.ulg", "--output", "converted"],
        vec!["flight.ulg", "--export-parquet", "implicit"],
        vec!["analyze", "flight.ulg", "--export-parquet", "explicit"],
    ] {
        let output = run(root.path(), &args);
        success(&output);
        let record = &records(&output)[0];
        assert_eq!(record["outcome"], "exported");
        assert!(record["modules"]["pid_step_response"].is_object());
        assert_ne!(record["modules"]["pid_step_response"]["status"], "not_run");
        validate_export(&root.path().join(record["export"].as_str().unwrap()));
    }
}

#[test]
fn directory_export_preserves_relative_filenames_and_index_errors() {
    let root = workspace();
    for path in ["logs/a/flight.ulg", "logs/b/flight.ulg", "logs/b/other.ULG"] {
        copy_log(&root.path().join(path));
    }
    fs::write(root.path().join("logs/bad.ulg"), b"bad").unwrap();
    let output = run(
        root.path(),
        &["convert", "logs", "--output", "export", "--jobs", "2"],
    );
    assert!(!output.status.success());
    assert_eq!(records(&output).len(), 4);
    for relative in ["a/flight.ulg", "b/flight.ulg", "b/other.%55%4C%47"] {
        validate_export(&root.path().join("export").join(relative));
    }
    let index: Value =
        serde_json::from_slice(&fs::read(root.path().join("export/index.json")).unwrap()).unwrap();
    assert_eq!(index["total"], 4);
    assert_eq!(index["logs"][0]["path"], "a/flight.ulg");
    assert!(index["logs"][0]["diagnostic_count"].is_number());
    assert_eq!(
        index["logs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["manifest"].is_string())
            .count(),
        3
    );
    assert_eq!(index["logs"][3]["outcome"], "error");
}

#[test]
fn filters_options_and_jobs_are_validated_before_processing() {
    let root = workspace();
    for args in [
        vec![
            "missing.ulg",
            "--modules",
            "unknown",
            "--export-parquet",
            "export",
        ],
        vec!["missing.ulg", "--analyzer", "unknown"],
        vec!["missing.ulg", "--exclude", "pid_step_response,unknown"],
        vec!["missing.ulg", "--format", "table"],
        vec!["missing.ulg", "--format", "text"],
        vec!["list", "--format", "table"],
        vec!["missing.ulg", "--format", "json"],
        vec!["missing.ulg", "--format", "json-pretty"],
        vec!["analyze", "missing.ulg", "--format", "json-pretty"],
        vec![
            "convert",
            "missing.ulg",
            "--output",
            "export",
            "--format",
            "json-pretty",
        ],
        vec!["list", "--format", "json-pretty"],
        vec!["missing.ulg", "--analyzer", "motor_failure,rc_loss"],
        vec![
            "missing.ulg",
            "--analyzer",
            "rc_loss",
            "--exclude",
            "pid_step_response",
        ],
        vec!["missing.ulg", "--jobs", "0"],
        vec!["missing.ulg", "--jobs", "257"],
        vec!["missing.ulg", "--jobs", "-1"],
        vec!["missing.ulg", "--output", "export"],
        vec!["convert", "missing.ulg"],
        vec![
            "convert",
            "missing.ulg",
            "--output",
            "export",
            "--export-parquet",
            "other",
        ],
        vec!["missing.ulg", "--unknown"],
        vec!["--modules", "unknown", "analyze", "missing.ulg"],
    ] {
        let output = run(root.path(), &args);
        assert!(!output.status.success(), "{args:?}");
        assert!(
            output.stdout.is_empty(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn selecting_pid_alone_runs_it_and_explains_unavailable_data() {
    let root = workspace();
    copy_log(&root.path().join("flight.ulg"));
    let output = run(
        root.path(),
        &["flight.ulg", "--analyzer", "pid_step_response"],
    );
    success(&output);
    let record = &records(&output)[0];
    assert_eq!(
        record["modules"]["pid_step_response"]["status"],
        "unavailable"
    );
    assert!(record["modules"]["pid_step_response"]["message"]
        .as_str()
        .unwrap()
        .contains("No axis met PID step-response criteria"));
    assert!(record["diagnostic_outcomes"]
        .as_object()
        .unwrap()
        .is_empty());
    assert!(record["analysis"]["diagnostics"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn exclusions_apply_to_diagnostics_and_pid_in_files_and_directories() {
    let root = workspace();
    copy_log(&root.path().join("logs/flight.ulg"));
    for input in ["logs/flight.ulg", "logs"] {
        let output = run(
            root.path(),
            &[input, "--exclude", "pid_step_response,rc_loss"],
        );
        success(&output);
        let record = &records(&output)[0];
        assert!(record["modules"].as_object().unwrap().is_empty());
        assert!(record["diagnostic_outcomes"].get("rc_loss").is_none());
        assert_eq!(
            record["diagnostic_outcomes"].as_object().unwrap().len(),
            flight_review::diagnostics::create_analyzers().len() - 1
        );
        let only = run(root.path(), &[input, "--analyzer", "rc_loss"]);
        success(&only);
        let record = &records(&only)[0];
        assert!(record["modules"].as_object().unwrap().is_empty());
        assert_eq!(record["diagnostic_outcomes"].as_object().unwrap().len(), 1);
        assert!(record["diagnostic_outcomes"].get("rc_loss").is_some());
    }
}

#[test]
fn analyzer_selection_reaches_exported_metadata_and_manifest() {
    let root = workspace();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tecs_nonfinite_pitch.ulg"),
        root.path().join("flight.ulg"),
    )
    .unwrap();
    for (option, id, destination) in [
        ("--analyzer", "pid_step_response", "pid-only"),
        ("--exclude", "tecs_nonfinite_pitch", "no-tecs"),
        ("--analyzer", "tecs_nonfinite_pitch", "tecs-only"),
    ] {
        let output = run(
            root.path(),
            &["convert", "flight.ulg", "--output", destination, option, id],
        );
        success(&output);
        let report = &records(&output)[0];
        let metadata: Value = serde_json::from_slice(
            &fs::read(root.path().join(destination).join("metadata.json")).unwrap(),
        )
        .unwrap();
        let manifest: Value = serde_json::from_slice(
            &fs::read(root.path().join(destination).join("manifest.json")).unwrap(),
        )
        .unwrap();
        for findings in [
            &report["analysis"]["diagnostics"],
            &metadata["analysis"]["diagnostics"],
            &manifest["diagnostics"],
        ] {
            let findings = findings.as_array().unwrap();
            if destination == "tecs-only" {
                assert!(!findings.is_empty());
                assert!(findings.iter().all(|d| d["id"] == "tecs_nonfinite_pitch"));
            } else {
                assert!(findings.iter().all(|d| d["id"] != "tecs_nonfinite_pitch"));
                if destination == "pid-only" {
                    assert!(findings.is_empty());
                }
            }
        }
    }
}

#[test]
fn excluding_every_analyzer_is_an_explicit_error() {
    let root = workspace();
    let ids = flight_review::diagnostics::create_analyzers()
        .iter()
        .map(|a| a.id().to_owned())
        .chain(
            flight_review::signal_processing::create_analyses()
                .iter()
                .map(|a| a.id().to_owned()),
        )
        .collect::<Vec<_>>()
        .join(",");
    let output = run(root.path(), &["missing.ulg", "--exclude", &ids]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no analyzers remain"));
    assert!(output.stdout.is_empty());
}

#[test]
fn single_and_directory_records_are_identical_compact_json() {
    let root = workspace();
    copy_log(&root.path().join("logs/flight.ulg"));
    let single = run(root.path(), &["logs/flight.ulg"]);
    let directory = run(root.path(), &["logs"]);
    success(&single);
    success(&directory);
    assert_eq!(records(&single), records(&directory));
    assert_eq!(String::from_utf8_lossy(&single.stdout).lines().count(), 1);
    assert_eq!(single.stdout, directory.stdout);
}

#[test]
fn list_and_help_advertise_only_canonical_commands() {
    let root = workspace();
    let help = run(root.path(), &["--help"]);
    success(&help);
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains(env!("CARGO_PKG_DESCRIPTION")));
    assert!(help.contains("analyze"));
    assert!(help.contains("convert"));
    assert!(help.contains("list"));
    assert!(!help.contains("batch"));
    assert!(!help.contains("--output "));
    assert!(!help.contains("--modules"));
    assert!(!help.contains("--format"));
    assert!(!help.contains("json-pretty"));
    assert!(help.contains("--exclude"));
    assert!(help.contains("All analyzers run by default"));
    let list = run(root.path(), &["list"]);
    success(&list);
    let catalog: Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(String::from_utf8_lossy(&list.stdout).lines().count(), 1);
    assert_eq!(catalog["version"], 1);
    let analyzers = catalog["analyzers"].as_array().unwrap();
    assert_eq!(
        analyzers.len(),
        flight_review::diagnostics::create_analyzers().len()
            + flight_review::signal_processing::create_analyses().len()
    );
    for id in [
        "motor_failure",
        "tecs_nonfinite_pitch",
        "ekf_selector_whipsaw",
        "pid_step_response",
    ] {
        assert!(analyzers
            .iter()
            .any(|analyzer| { analyzer["id"] == id && analyzer["description"].is_string() }));
    }
}

#[test]
fn recovered_truncated_prefix_remains_analyzable() {
    let root = workspace();
    let mut bytes = fs::read(fixture()).unwrap();
    bytes.truncate(bytes.len() - 3);
    fs::write(root.path().join("partial.ulg"), bytes).unwrap();
    let output = run(root.path(), &["partial.ulg"]);
    success(&output);
    assert_eq!(records(&output)[0]["summary"]["completeness"], "truncated");
    let exported = run(root.path(), &["partial.ulg", "--export-parquet", "export"]);
    success(&exported);
    assert_eq!(
        records(&exported)[0]["summary"]["completeness"],
        "truncated"
    );
}

#[test]
fn export_refuses_source_overlap_and_existing_data() {
    let root = workspace();
    copy_log(&root.path().join("logs/flight.ulg"));
    fs::create_dir(root.path().join("existing")).unwrap();
    fs::write(root.path().join("existing/keep"), b"safe").unwrap();
    fs::write(root.path().join("file"), b"safe").unwrap();
    for output in [
        "logs",
        "logs/nested",
        ".",
        "existing",
        "file",
        "logs/flight.ulg",
    ] {
        let result = run(root.path(), &["convert", "logs", "--output", output]);
        assert!(!result.status.success(), "{output}");
    }
    assert_eq!(
        fs::read(root.path().join("existing/keep")).unwrap(),
        b"safe"
    );
    assert_eq!(fs::read(root.path().join("file")).unwrap(), b"safe");
    assert_eq!(fs::read_dir(root.path().join("logs")).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn symlinks_are_not_followed_or_written_through() {
    use std::os::unix::fs::symlink;
    let root = workspace();
    copy_log(&root.path().join("logs/flight.ulg"));
    copy_log(&root.path().join("outside/other.ulg"));
    symlink("../outside", root.path().join("logs/link")).unwrap();
    symlink("../outside/other.ulg", root.path().join("logs/link.ulg")).unwrap();
    let output = run(root.path(), &["logs"]);
    success(&output);
    assert_eq!(records(&output).len(), 1);
    symlink("outside", root.path().join("export")).unwrap();
    let output = run(
        root.path(),
        &["convert", "logs", "--output", "export/nested"],
    );
    assert!(!output.status.success());
    assert!(!root.path().join("outside/nested").exists());
}

#[cfg(unix)]
#[test]
fn read_and_walk_failures_are_not_ignored() {
    use std::os::unix::fs::PermissionsExt;
    let root = workspace();
    copy_log(&root.path().join("logs/good.ulg"));
    copy_log(&root.path().join("logs/unreadable.ulg"));
    let unreadable = root.path().join("logs/unreadable.ulg");
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o0)).unwrap();
    if fs::read(&unreadable).is_err() {
        let output = run(root.path(), &["logs"]);
        assert!(!output.status.success());
        let rows = records(&output);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1]["outcome"], "error");
    }
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o600)).unwrap();
    let directory = root.path().join("logs/locked");
    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o0)).unwrap();
    if fs::read_dir(&directory).is_err() {
        let output = run(root.path(), &["logs"]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("locked"));
    }
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn broken_stdout_is_a_reported_failure_not_a_panic() {
    use std::process::Stdio;
    let root = workspace();
    copy_log(&root.path().join("flight.ulg"));
    let mut child = Command::new(env!("CARGO_BIN_EXE_flight-review"))
        .current_dir(root.path())
        .args(["flight.ulg"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take());
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));
}

#[test]
fn nonfinite_metadata_is_an_explicit_export_error() {
    let root = workspace();
    let mut bytes = fs::read(fixture()).unwrap();
    let key = b"float CLI_TEST";
    let size = 1 + key.len() + 4;
    bytes.extend_from_slice(&(size as u16).to_le_bytes());
    bytes.push(b'P');
    bytes.push(key.len() as u8);
    bytes.extend_from_slice(key);
    bytes.extend_from_slice(&f32::NAN.to_le_bytes());
    fs::write(root.path().join("nonfinite.ulg"), bytes).unwrap();
    let output = run(
        root.path(),
        &["convert", "nonfinite.ulg", "--output", "export"],
    );
    assert!(!output.status.success());
    let record = &records(&output)[0];
    assert_eq!(record["outcome"], "error");
    assert!(record["error"].as_str().unwrap().contains("nonfinite"));
}

fn message(log: &mut Vec<u8>, kind: u8, payload: &[u8]) {
    log.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    log.push(kind);
    log.extend_from_slice(payload);
}

fn write_gps_log(path: &Path, coordinates: &[[f64; 3]]) {
    let mut log = b"ULog\x01\x12\x35\x01".to_vec();
    log.extend_from_slice(&1_000_000_u64.to_le_bytes());
    message(&mut log, b'B', &[0; 40]);
    message(
        &mut log,
        b'F',
        b"vehicle_gps_position:uint64_t timestamp;double latitude_deg;double longitude_deg;double altitude_msl_m;",
    );
    let mut subscription = vec![0, 0, 0];
    subscription.extend_from_slice(b"vehicle_gps_position");
    message(&mut log, b'A', &subscription);
    for (index, values) in coordinates.iter().enumerate() {
        let mut payload = 0_u16.to_le_bytes().to_vec();
        payload.extend_from_slice(&(1_000_000 + index as u64 * 100_000).to_le_bytes());
        for value in values {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        message(&mut log, b'D', &payload);
    }
    fs::write(path, log).unwrap();
}

fn write_pid_log(path: &Path) {
    let mut log = b"ULog\x01\x12\x35\x01".to_vec();
    log.extend_from_slice(&1_000_000_u64.to_le_bytes());
    message(&mut log, b'B', &[0; 40]);
    let topics = [
        (
            0_u16,
            "vehicle_rates_setpoint",
            "vehicle_rates_setpoint:uint64_t timestamp;float roll;float pitch;float yaw;",
        ),
        (
            1,
            "vehicle_angular_velocity",
            "vehicle_angular_velocity:uint64_t timestamp;float[3] xyz;",
        ),
    ];
    for (_, _, format) in &topics {
        message(&mut log, b'F', format.as_bytes());
    }
    for (id, name, _) in topics {
        let mut subscription = vec![0];
        subscription.extend_from_slice(&id.to_le_bytes());
        subscription.extend_from_slice(name.as_bytes());
        message(&mut log, b'A', &subscription);
    }
    let mut state = 0x1234_5678_u32;
    for i in 0..1200 {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let value = (2.0 * state as f64 / u32::MAX as f64 - 1.0) as f32;
        for id in [0_u16, 1] {
            let mut payload = id.to_le_bytes().to_vec();
            payload.extend_from_slice(&(1_000_000 + i * 10_000_u64).to_le_bytes());
            for _ in 0..3 {
                payload.extend_from_slice(&value.to_le_bytes());
            }
            message(&mut log, b'D', &payload);
        }
    }
    fs::write(path, log).unwrap();
}

#[test]
fn default_file_directory_and_convert_execute_pid_without_opt_in() {
    let root = workspace();
    fs::create_dir(root.path().join("logs")).unwrap();
    write_pid_log(&root.path().join("logs/flight.ulg"));
    for args in [
        vec!["logs/flight.ulg"],
        vec!["logs"],
        vec!["analyze", "logs"],
        vec!["convert", "logs", "--output", "export"],
    ] {
        let output = run(root.path(), &args);
        success(&output);
        let record = &records(&output)[0];
        let pid = &record["modules"]["pid_step_response"];
        assert_eq!(pid["status"], "completed", "{pid}");
        let axes = pid["result"]["axes"].as_array().unwrap();
        assert_eq!(axes.len(), 3);
        for axis in axes {
            assert!(axis["window_count"].as_u64().unwrap() >= 3);
            assert!(axis["mean_response"]
                .as_array()
                .unwrap()
                .iter()
                .all(|value| (value.as_f64().unwrap() - 1.0).abs() < 0.12));
        }
    }
    let excluded = run(root.path(), &["logs", "--exclude", "pid_step_response"]);
    success(&excluded);
    assert!(records(&excluded)[0]["modules"]
        .as_object()
        .unwrap()
        .is_empty());
    let only = run(root.path(), &["logs", "--analyzer", "pid_step_response"]);
    success(&only);
    assert_eq!(
        records(&only)[0]["modules"]["pid_step_response"]["status"],
        "completed"
    );
    assert!(records(&only)[0]["diagnostic_outcomes"]
        .as_object()
        .unwrap()
        .is_empty());
}

fn assert_gps_exports(root: &Path, expected: Value) {
    for arguments in [
        vec!["convert", "gps.ulg", "--output", "converted"],
        vec!["gps.ulg", "--export-parquet", "analyzed"],
    ] {
        let output = run(root, &arguments);
        success(&output);
        let export = root.join(records(&output)[0]["export"].as_str().unwrap());
        validate_export(&export);
        let metadata: Value =
            serde_json::from_slice(&fs::read(export.join("metadata.json")).unwrap()).unwrap();
        assert_eq!(metadata["gps_first_fix"], expected);
    }
}

#[test]
fn gps_metadata_waits_for_first_finite_position() {
    let root = workspace();
    write_gps_log(
        &root.path().join("gps.ulg"),
        &[
            [f64::NAN, 8.5, 450.0],
            [47.5, f64::INFINITY, 450.0],
            [47.5, 8.5, f64::NAN],
            [47.5, 8.5, f64::NEG_INFINITY],
            [47.5, 8.5, 450.0],
            [48.0, 9.0, 460.0],
        ],
    );
    assert_gps_exports(
        root.path(),
        serde_json::json!({
            "lat_deg": 47.5, "lon_deg": 8.5, "alt_m": 450.0,
        }),
    );
}

#[test]
fn all_unavailable_gps_positions_export_as_missing_metadata() {
    let root = workspace();
    write_gps_log(
        &root.path().join("gps.ulg"),
        &[
            [f64::NAN, 8.5, 450.0],
            [47.5, f64::NAN, 450.0],
            [47.5, 8.5, f64::INFINITY],
        ],
    );
    assert_gps_exports(root.path(), Value::Null);
}

#[test]
fn directory_exports_reserve_root_index_without_colliding_with_literal_escapes() {
    let root = workspace();
    for relative in [
        "index.json/flight.ulg",
        "%69ndex.json/flight.ulg",
        "nested/index.json/flight.ulg",
    ] {
        copy_log(&root.path().join("logs").join(relative));
    }
    let output = run(root.path(), &["convert", "logs", "--output", "export"]);
    success(&output);
    let index: Value =
        serde_json::from_slice(&fs::read(root.path().join("export/index.json")).unwrap()).unwrap();
    assert_eq!(index["total"], 3);
    let entries = index["logs"].as_array().unwrap();
    for (source, expected) in [
        ("logs/index.json/flight.ulg", "%69ndex.json/flight.ulg"),
        ("logs/%69ndex.json/flight.ulg", "%2569ndex.json/flight.ulg"),
        (
            "logs/nested/index.json/flight.ulg",
            "nested/index.json/flight.ulg",
        ),
    ] {
        let entry = entries
            .iter()
            .find(|entry| entry["file"] == source)
            .unwrap();
        assert_eq!(entry["path"], expected);
        assert_eq!(entry["manifest"], format!("{expected}/manifest.json"));
        validate_export(&root.path().join("export").join(expected));
        assert_eq!(
            fs::read(root.path().join(source)).unwrap(),
            fs::read(fixture()).unwrap()
        );
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_log_paths_fail_individually_in_analysis_and_export_modes() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let root = workspace();
    copy_log(&root.path().join("logs/good.ulg"));
    let invalid = root
        .path()
        .join("logs")
        .join(OsStr::from_bytes(b"bad\xff.ulg"));
    if let Err(error) = fs::write(&invalid, fs::read(fixture()).unwrap()) {
        // APFS rejects non-UTF-8 filenames; Linux filesystems exercise this case.
        if cfg!(target_os = "macos") && error.raw_os_error() == Some(92) {
            eprintln!("Skipping non-UTF-8 filename regression: filesystem rejects invalid UTF-8");
            return;
        }
        panic!("could not create non-UTF-8 fixture: {error}");
    }
    for (arguments, export) in [
        (vec!["logs"], None),
        (vec!["analyze", "logs"], None),
        (
            vec!["logs", "--export-parquet", "implicit"],
            Some("implicit"),
        ),
        (
            vec!["analyze", "logs", "--export-parquet", "explicit"],
            Some("explicit"),
        ),
        (
            vec!["convert", "logs", "--output", "converted"],
            Some("converted"),
        ),
    ] {
        let output = run(root.path(), &arguments);
        assert!(!output.status.success());
        let rows = records(&output);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["outcome"], "error");
        assert!(rows[0]["error"].as_str().unwrap().contains("UTF-8"));
        assert_eq!(rows[1]["file"], "logs/good.ulg");
        assert_eq!(
            rows[1]["outcome"],
            if export.is_some() {
                "exported"
            } else {
                "analyzed"
            }
        );
        if let Some(export) = export {
            let directory = root.path().join(export);
            validate_export(&directory.join("good.ulg"));
            let index: Value =
                serde_json::from_slice(&fs::read(directory.join("index.json")).unwrap()).unwrap();
            assert_eq!(index["total"], 2);
            assert_eq!(index["logs"][0]["outcome"], "error");
            assert!(index["logs"][0]["path"].is_null());
            assert!(index["logs"][0]["manifest"].is_null());
            assert_eq!(index["logs"][1]["path"], "good.ulg");
        }
    }
}

#[test]
fn missing_any_required_diagnostic_topic_is_unavailable() {
    let root = workspace();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.ulg"),
        root.path().join("sample.ulg"),
    )
    .unwrap();
    let output = run(root.path(), &["sample.ulg"]);
    success(&output);
    let rows = records(&output);
    for (id, topic) in [
        ("battery_brownout", "battery_status"),
        ("rc_loss", "input_rc"),
    ] {
        let outcome = &rows[0]["diagnostic_outcomes"][id];
        assert_eq!(outcome["status"], "unavailable");
        assert!(outcome["message"].as_str().unwrap().contains(topic));
    }
    copy_log(&root.path().join("motor.ulg"));
    let output = run(root.path(), &["motor.ulg", "--analyzer", "motor_failure"]);
    success(&output);
    assert_eq!(
        records(&output)[0]["diagnostic_outcomes"]["motor_failure"]["status"],
        "no_findings"
    );
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tecs_nonfinite_pitch.ulg"),
        root.path().join("tecs.ulg"),
    )
    .unwrap();
    let output = run(
        root.path(),
        &["tecs.ulg", "--analyzer", "tecs_nonfinite_pitch"],
    );
    success(&output);
    assert_eq!(
        records(&output)[0]["diagnostic_outcomes"]["tecs_nonfinite_pitch"]["status"],
        "findings"
    );
}
