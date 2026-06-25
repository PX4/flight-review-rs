//! Capture git SHA and build time at compile time so the server can report
//! exactly which build is running (see the `/api/version` endpoint).
//!
//! Everything here degrades to `"unknown"` rather than failing: builds happen
//! in plenty of places without git (release tarballs, `cargo install`, vendored
//! sources), and a missing debug string must never break the build.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("cargo:rustc-env=GIT_SHA={}", git_sha());
    println!("cargo:rustc-env=BUILD_TIME={}", build_time());
    // Refresh the SHA when HEAD moves. Best-effort: absent in non-git builds.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
}

/// Short HEAD SHA, with a `-dirty` suffix when the working tree has changes.
fn git_sha() -> String {
    let sha = run_git(&["rev-parse", "--short", "HEAD"]);
    let Some(sha) = sha else {
        return "unknown".to_string();
    };
    // `--porcelain` prints nothing for a clean tree.
    let dirty = run_git(&["status", "--porcelain"])
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    if dirty {
        format!("{sha}-dirty")
    } else {
        sha
    }
}

fn run_git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    // Empty output is meaningful for some callers (e.g. a clean `status
    // --porcelain`), so return it as-is rather than treating it as failure.
    Some(String::from_utf8(out.stdout).ok()?.trim().to_string())
}

/// Build time as an ISO-8601 UTC string. Honors `SOURCE_DATE_EPOCH` for
/// reproducible builds; otherwise uses the wall clock. Formatted without a date
/// library to avoid a build-dependency.
fn build_time() -> String {
    let secs = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs())
        });
    match secs {
        Some(secs) => format_iso8601_utc(secs),
        None => "unknown".to_string(),
    }
}

/// Format Unix seconds as `YYYY-MM-DDTHH:MM:SSZ` (UTC, proleptic Gregorian).
fn format_iso8601_utc(secs: u64) -> String {
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (hour, min, sec) = (rem / 3600, (rem % 3600) / 60, rem % 60);

    // Civil-from-days (Howard Hinnant's algorithm), epoch 1970-01-01.
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}
