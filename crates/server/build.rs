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
/// reproducible builds; otherwise uses the wall clock. `unknown` if neither is
/// available.
fn build_time() -> String {
    let secs = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .ok()
                .map(|d| d.as_secs() as i64)
        });
    secs.and_then(|s| chrono::DateTime::from_timestamp(s, 0))
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_else(|| "unknown".to_string())
}
