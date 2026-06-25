//! Capture the resolved `px4-ulog` version at build time so it can be surfaced
//! at runtime (e.g. in the dashboard's version panel).
//!
//! Cargo offers no direct env var for a dependency's *resolved* version (the
//! `DEP_<crate>` mechanism only exists for `links =` crates, which px4-ulog is
//! not, and the Cargo.toml requirement string is the requirement, not the
//! resolved version). The committed `Cargo.lock` records the exact resolved
//! version, so we read it here. Any failure falls back to `unknown` — the build
//! must never break over a debug string.

use std::path::{Path, PathBuf};

fn main() {
    let version = resolve_px4_ulog_version().unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=PX4_ULOG_VERSION={version}");
}

/// Walk up from the crate manifest dir to find the workspace `Cargo.lock` and
/// read the `px4-ulog` package version out of it.
fn resolve_px4_ulog_version() -> Option<String> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let lock_path = find_cargo_lock(Path::new(&manifest_dir))?;
    println!("cargo:rerun-if-changed={}", lock_path.display());

    let contents = std::fs::read_to_string(&lock_path).ok()?;
    let doc: toml::Value = contents.parse().ok()?;
    let packages = doc.get("package")?.as_array()?;
    packages
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("px4-ulog"))
        .and_then(|p| p.get("version"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Find the workspace `Cargo.lock` by walking up from `start` and keeping the
/// *outermost* lockfile found. A stale per-crate `crates/converter/Cargo.lock`
/// can exist and resolve an old version, so the nearest lock is the wrong one;
/// the authoritative resolution lives at the workspace root.
fn find_cargo_lock(start: &Path) -> Option<PathBuf> {
    let mut outermost = None;
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join("Cargo.lock");
        if candidate.is_file() {
            outermost = Some(candidate);
        }
        dir = d.parent();
    }
    outermost
}
