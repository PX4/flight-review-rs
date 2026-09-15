pub mod analysis;
pub mod cli_support;
pub mod converter;
pub mod diagnostics;
pub mod metadata;
pub mod signal_processing;

/// This crate's version (shared library and `flight-review` CLI).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The resolved `px4-ulog` parser version, captured from `Cargo.lock` at build
/// time (see `build.rs`). `"unknown"` if it couldn't be determined.
pub const PX4_ULOG_VERSION: &str = env!("PX4_ULOG_VERSION");
