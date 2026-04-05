//! Signal processing analysis framework.
//!
//! Provides a trait-based system for running signal processing analyses on
//! ULog flight data. Unlike the streaming diagnostic analyzers, signal
//! processing modules need full time-series vectors for FFT, spectral
//! analysis, and deconvolution.
//!
//! ## Architecture
//!
//! ```text
//! Phase 1: Extract signals (single ULog pass)
//!   → SignalStore { topic.field → Vec<(timestamp_s, value)> }
//!
//! Phase 2: Run analyses (per module, on extracted data)
//!   → HashMap<module_id, JSON result>
//! ```
//!
//! ## Adding a new analysis module
//!
//! 1. Create `crates/converter/src/signal_processing/your_module.rs`
//! 2. Implement [`SignalAnalysis`] — declare signals, run analysis
//! 3. Register in [`create_analyses()`]
//! 4. Add tests following the pattern in [`testing`]

use px4_ulog::stream_parser::file_reader::{
    read_file_with_simple_callback, Message, SimpleCallbackResult,
};
use std::collections::{HashMap, HashSet};

pub mod dsp;
pub mod pid_step_response;
#[cfg(test)]
pub mod testing;

// Re-export result types for backward compatibility
pub use pid_step_response::{PidAnalysisResult, PidStepResponse, StepResponseHistogram};

/// A time series of (timestamp_seconds, value) pairs.
pub type TimeSeries = Vec<(f64, f64)>;

/// Identifies a signal to extract from a ULog file.
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct SignalRequest {
    pub topic: String,
    pub field: String,
}

impl SignalRequest {
    pub fn new(topic: &str, field: &str) -> Self {
        Self {
            topic: topic.to_string(),
            field: field.to_string(),
        }
    }
}

/// Holds extracted time-series signals keyed by (topic, field).
pub struct SignalStore {
    signals: HashMap<SignalRequest, TimeSeries>,
}

impl SignalStore {
    /// Get a signal by request. Returns an empty slice if the signal
    /// was not found or had no data.
    pub fn get(&self, request: &SignalRequest) -> &[(f64, f64)] {
        self.signals
            .get(request)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

/// Errors that can occur during signal processing analysis.
#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    #[error("insufficient data: {reason}")]
    InsufficientData { reason: String },
    #[error("processing error: {0}")]
    Processing(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Trait that all signal processing modules implement.
pub trait SignalAnalysis: Send {
    /// Machine-readable identifier (e.g., "pid_step_response").
    fn id(&self) -> &str;

    /// Short human-readable description.
    fn description(&self) -> &str;

    /// Which signals this analysis needs. The framework extracts these
    /// from the ULog file in a single pass before calling `analyze()`.
    fn required_signals(&self) -> Vec<SignalRequest>;

    /// Run the analysis on extracted signals.
    fn analyze(&self, signals: &SignalStore) -> Result<serde_json::Value, AnalysisError>;
}

/// Create all registered signal processing analyses.
pub fn create_analyses() -> Vec<Box<dyn SignalAnalysis>> {
    vec![Box::new(pid_step_response::PidStepResponseAnalysis)]
}

/// Create only the analyses whose IDs are in the given list.
pub fn create_analyses_filtered(ids: &[String]) -> Result<Vec<Box<dyn SignalAnalysis>>, String> {
    let all = create_analyses();
    for id in ids {
        if !all.iter().any(|a| a.id() == id.as_str()) {
            let valid: Vec<&str> = all.iter().map(|a| a.id()).collect();
            return Err(format!(
                "unknown analysis '{}'. valid: {}",
                id,
                valid.join(", ")
            ));
        }
    }
    let selected = create_analyses()
        .into_iter()
        .filter(|a| ids.iter().any(|id| id == a.id()))
        .collect();
    Ok(selected)
}

/// Extract all requested signals from a ULog file in a single pass.
pub fn extract_signals(
    path: &str,
    requests: &HashSet<SignalRequest>,
) -> Result<SignalStore, std::io::Error> {
    // Build a topic → Vec<field> lookup for fast dispatch
    let mut topic_fields: HashMap<&str, Vec<&str>> = HashMap::new();
    for req in requests {
        topic_fields
            .entry(req.topic.as_str())
            .or_default()
            .push(req.field.as_str());
    }

    let mut signals: HashMap<SignalRequest, TimeSeries> = requests
        .iter()
        .map(|r| (r.clone(), Vec::new()))
        .collect();

    read_file_with_simple_callback(path, &mut |msg| {
        if let Message::Data(data) = msg {
            let topic = data.flattened_format.message_name.as_str();

            if let Some(fields) = topic_fields.get(topic) {
                let ts = data
                    .flattened_format
                    .timestamp_field
                    .as_ref()
                    .map(|tf| tf.parse_timestamp(data.data));

                if let Some(ts) = ts {
                    let t_s = ts as f64 / 1_000_000.0;

                    for &field in fields {
                        if let Ok(parser) =
                            data.flattened_format.get_field_parser::<f32>(field)
                        {
                            let val = parser.parse(data.data) as f64;
                            if val.is_finite() {
                                let key = SignalRequest::new(topic, field);
                                if let Some(series) = signals.get_mut(&key) {
                                    series.push((t_s, val));
                                }
                            }
                        }
                    }
                }
            }
        }
        SimpleCallbackResult::KeepReading
    })?;

    // Sort each signal by timestamp
    for series in signals.values_mut() {
        series.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    }

    Ok(SignalStore { signals })
}

/// Run selected analyses on a ULog file.
///
/// Extracts all required signals in a single pass, then runs each module.
/// Modules that return `InsufficientData` are silently skipped.
pub fn run_analyses(
    path: &str,
    analyses: &[Box<dyn SignalAnalysis>],
) -> Result<HashMap<String, serde_json::Value>, AnalysisError> {
    // Collect all required signals
    let all_requests: HashSet<SignalRequest> = analyses
        .iter()
        .flat_map(|a| a.required_signals())
        .collect();

    // Single extraction pass
    let store = extract_signals(path, &all_requests)?;

    // Run each analysis
    let mut results = HashMap::new();
    for analysis in analyses {
        match analysis.analyze(&store) {
            Ok(result) => {
                results.insert(analysis.id().to_string(), result);
            }
            Err(AnalysisError::InsufficientData { .. }) => {
                // Silently skip — not all logs have all signals
            }
            Err(e) => return Err(e),
        }
    }

    Ok(results)
}
