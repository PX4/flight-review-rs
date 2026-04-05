//! PID step response analysis — backward-compatible entry point.
//!
//! This module delegates to [`crate::signal_processing::pid_step_response`].
//! Use [`pid_analysis()`] for the legacy API, or the signal processing
//! framework directly via [`crate::signal_processing::run_analyses()`].

// Re-export types for backward compatibility
pub use crate::signal_processing::pid_step_response::{
    PidAnalysisResult, PidStepResponse, StepResponseHistogram,
};

/// Run PID step response analysis on a ULog file.
///
/// This is a convenience wrapper around the signal processing framework.
/// Returns results for each axis that has sufficient data.
pub fn pid_analysis(path: &str) -> Result<PidAnalysisResult, std::io::Error> {
    let analyses = crate::signal_processing::create_analyses();
    let pid_only: Vec<_> = analyses
        .into_iter()
        .filter(|a| a.id() == "pid_step_response")
        .collect();

    let results = crate::signal_processing::run_analyses(path, &pid_only)
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    match results.get("pid_step_response") {
        Some(value) => {
            let result: PidAnalysisResult = serde_json::from_value(value.clone())
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            Ok(result)
        }
        None => Ok(PidAnalysisResult { axes: Vec::new() }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px4_ulog_fixture(name: &str) -> String {
        crate::signal_processing::testing::fixture_path(name)
    }

    #[test]
    fn test_pid_analysis_sample() {
        let path = px4_ulog_fixture("sample.ulg");
        let result = pid_analysis(&path).unwrap();
        for axis_result in &result.axes {
            assert!(axis_result.sample_rate_hz >= 50.0);
            assert!(axis_result.window_count >= 3);
            assert!(!axis_result.time_s.is_empty());
            assert_eq!(axis_result.time_s.len(), axis_result.mean_response.len());
            assert_eq!(axis_result.histogram.time_bins.len(), 100);
            assert_eq!(axis_result.histogram.amplitude_bins.len(), 100);
            assert_eq!(axis_result.histogram.counts.len(), 100 * 100);
        }
    }

    #[test]
    fn test_pid_analysis_fixed_wing() {
        let path = px4_ulog_fixture("fixed_wing_gps.ulg");
        if !std::path::Path::new(&path).exists() {
            eprintln!("Skipping: fixed_wing_gps.ulg not available");
            return;
        }
        let result = pid_analysis(&path).unwrap();
        for axis_result in &result.axes {
            assert!(axis_result.window_count >= 3);
            assert!(!axis_result.mean_response.is_empty());
        }
    }
}
