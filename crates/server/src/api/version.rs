//! `GET /api/version` — reports the running versions of the server, converter
//! library, and px4-ulog parser, plus the server's git SHA and build time.
//!
//! Everything is captured at compile time (see the server and converter
//! `build.rs` scripts), so the handler is static: no state, no DB, no auth.
//! The frontend reports its own version separately (baked at its build time),
//! which lets independently-deployed front/back drift be diagnosed.

use axum::Json;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct VersionInfo {
    /// HTTP server crate version.
    pub server: &'static str,
    /// Converter/analysis library version.
    pub converter: &'static str,
    /// Resolved px4-ulog parser version.
    pub px4_ulog: &'static str,
    /// Short git SHA the server was built from (`-dirty` if the tree had
    /// uncommitted changes, `unknown` for non-git builds).
    pub git_sha: &'static str,
    /// Server build time, ISO-8601 UTC (`unknown` if unavailable).
    pub build_time: &'static str,
}

pub async fn version() -> Json<VersionInfo> {
    Json(VersionInfo {
        server: env!("CARGO_PKG_VERSION"),
        converter: flight_review::VERSION,
        px4_ulog: flight_review::PX4_ULOG_VERSION,
        git_sha: env!("GIT_SHA"),
        build_time: env!("BUILD_TIME"),
    })
}
