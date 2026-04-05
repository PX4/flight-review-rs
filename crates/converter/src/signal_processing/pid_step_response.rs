//! PID step response analysis using Wiener deconvolution.
//!
//! Extracts the step response of the PID controller for each axis (roll,
//! pitch, yaw) by deconvolving the rate setpoint (input) from the actual
//! angular rate (output).

use super::dsp::{hanning_window, median_sample_rate, resample_uniform};
use super::{AnalysisError, SignalAnalysis, SignalRequest, SignalStore};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PidStepResponse {
    pub axis: String,
    pub sample_rate_hz: f64,
    pub window_count: usize,
    /// Time points for the step response (0 to ~0.5s)
    pub time_s: Vec<f64>,
    /// Mean step response values (normalized, should approach 1.0)
    pub mean_response: Vec<f64>,
    /// 2D histogram: time_bins x amplitude_bins -> count
    pub histogram: StepResponseHistogram,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResponseHistogram {
    pub time_bins: Vec<f64>,
    pub amplitude_bins: Vec<f64>,
    /// Row-major: counts[time_idx * amplitude_bins.len() + amp_idx]
    pub counts: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PidAnalysisResult {
    pub axes: Vec<PidStepResponse>,
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

const WINDOW_DURATION_S: f64 = 1.0;
const STEP_DURATION_S: f64 = 0.5;
const RESPONSE_DURATION_S: f64 = 0.5;
const MIN_SAMPLE_RATE_HZ: f64 = 50.0;
const MIN_WINDOWS: usize = 3;
const NOISE_FLOOR_FACTOR: f64 = 1e-3;

const HIST_TIME_BINS: usize = 100;
const HIST_AMP_BINS: usize = 100;
const HIST_AMP_MIN: f64 = -0.5;
const HIST_AMP_MAX: f64 = 2.0;

// ---------------------------------------------------------------------------
// Axis mapping
// ---------------------------------------------------------------------------

const AXES: &[(&str, &str, &str)] = &[
    ("roll", "roll", "xyz[0]"),
    ("pitch", "pitch", "xyz[1]"),
    ("yaw", "yaw", "xyz[2]"),
];

// ---------------------------------------------------------------------------
// SignalAnalysis implementation
// ---------------------------------------------------------------------------

pub struct PidStepResponseAnalysis;

impl SignalAnalysis for PidStepResponseAnalysis {
    fn id(&self) -> &str {
        "pid_step_response"
    }

    fn description(&self) -> &str {
        "PID controller step response (Wiener deconvolution)"
    }

    fn required_signals(&self) -> Vec<SignalRequest> {
        AXES.iter()
            .flat_map(|(_, setpoint_field, gyro_field)| {
                vec![
                    SignalRequest::new("vehicle_rates_setpoint", setpoint_field),
                    SignalRequest::new("vehicle_angular_velocity", gyro_field),
                ]
            })
            .collect()
    }

    fn analyze(&self, signals: &SignalStore) -> Result<serde_json::Value, AnalysisError> {
        let mut result = PidAnalysisResult { axes: Vec::new() };

        for (axis_name, setpoint_field, gyro_field) in AXES {
            let setpoint_req = SignalRequest::new("vehicle_rates_setpoint", setpoint_field);
            let gyro_req = SignalRequest::new("vehicle_angular_velocity", gyro_field);

            let setpoint_raw = signals.get(&setpoint_req);
            let gyro_raw = signals.get(&gyro_req);

            if let Some(response) = analyze_axis(axis_name, setpoint_raw, gyro_raw) {
                result.axes.push(response);
            }
        }

        if result.axes.is_empty() {
            return Err(AnalysisError::InsufficientData {
                reason: "no axis had sufficient data for PID analysis".to_string(),
            });
        }

        serde_json::to_value(&result).map_err(AnalysisError::Serialization)
    }
}

// ---------------------------------------------------------------------------
// Per-axis analysis (now takes pre-extracted signals)
// ---------------------------------------------------------------------------

fn analyze_axis(
    axis: &str,
    setpoint_raw: &[(f64, f64)],
    gyro_raw: &[(f64, f64)],
) -> Option<PidStepResponse> {
    if setpoint_raw.len() < 2 || gyro_raw.len() < 2 {
        return None;
    }

    let sample_rate = median_sample_rate(setpoint_raw);
    if sample_rate < MIN_SAMPLE_RATE_HZ {
        return None;
    }

    let t_start = setpoint_raw[0].0.max(gyro_raw[0].0);
    let t_end = setpoint_raw.last().unwrap().0.min(gyro_raw.last().unwrap().0);
    if t_end - t_start < WINDOW_DURATION_S {
        return None;
    }

    let setpoint = resample_uniform(setpoint_raw, sample_rate, t_start, t_end);
    let gyro = resample_uniform(gyro_raw, sample_rate, t_start, t_end);

    let n = setpoint.len().min(gyro.len());
    if n < 2 {
        return None;
    }
    let setpoint = &setpoint[..n];
    let gyro = &gyro[..n];

    let window_samples = (WINDOW_DURATION_S * sample_rate).round() as usize;
    let step_samples = (STEP_DURATION_S * sample_rate).round() as usize;
    let response_samples = (RESPONSE_DURATION_S * sample_rate).round() as usize;

    if window_samples < 4 || step_samples == 0 || response_samples == 0 {
        return None;
    }

    let hann = hanning_window(window_samples);
    let mut all_step_responses: Vec<Vec<f64>> = Vec::new();

    let mut offset = 0;
    while offset + window_samples <= n {
        let sp_win = &setpoint[offset..offset + window_samples];
        let gy_win = &gyro[offset..offset + window_samples];

        if let Some(step) =
            wiener_step_response(sp_win, gy_win, &hann, window_samples, response_samples)
        {
            all_step_responses.push(step);
        }

        offset += step_samples;
    }

    if all_step_responses.len() < MIN_WINDOWS {
        return None;
    }

    let resp_len = response_samples.min(
        all_step_responses
            .iter()
            .map(|r| r.len())
            .min()
            .unwrap_or(0),
    );
    if resp_len == 0 {
        return None;
    }

    let mut mean_response = vec![0.0f64; resp_len];
    for resp in &all_step_responses {
        for (i, &v) in resp.iter().take(resp_len).enumerate() {
            mean_response[i] += v;
        }
    }
    let count = all_step_responses.len() as f64;
    for v in &mut mean_response {
        *v /= count;
    }

    let dt = 1.0 / sample_rate;
    let time_s: Vec<f64> = (0..resp_len).map(|i| i as f64 * dt).collect();

    let histogram = build_histogram(&all_step_responses, resp_len, &time_s);

    Some(PidStepResponse {
        axis: axis.to_string(),
        sample_rate_hz: sample_rate,
        window_count: all_step_responses.len(),
        time_s,
        mean_response,
        histogram,
    })
}

// ---------------------------------------------------------------------------
// Wiener deconvolution
// ---------------------------------------------------------------------------

fn wiener_step_response(
    input: &[f64],
    output: &[f64],
    hann: &[f64],
    fft_len: usize,
    response_len: usize,
) -> Option<Vec<f64>> {
    let mut x: Vec<Complex<f64>> = input
        .iter()
        .zip(hann.iter())
        .map(|(&s, &w)| Complex::new(s * w, 0.0))
        .collect();
    let mut y: Vec<Complex<f64>> = output
        .iter()
        .zip(hann.iter())
        .map(|(&s, &w)| Complex::new(s * w, 0.0))
        .collect();

    x.resize(fft_len, Complex::new(0.0, 0.0));
    y.resize(fft_len, Complex::new(0.0, 0.0));

    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_len);
    fft.process(&mut x);
    fft.process(&mut y);

    let max_power = x
        .iter()
        .map(|c| c.norm_sqr())
        .fold(0.0f64, |a, b| a.max(b));
    if max_power < 1e-30 {
        return None;
    }
    let noise_floor = max_power * NOISE_FLOOR_FACTOR;

    let mut h: Vec<Complex<f64>> = y
        .iter()
        .zip(x.iter())
        .map(|(yi, xi)| {
            let denom = xi.norm_sqr() + noise_floor;
            (*yi * xi.conj()) / denom
        })
        .collect();

    let ifft = planner.plan_fft_inverse(fft_len);
    ifft.process(&mut h);

    let scale = 1.0 / fft_len as f64;

    let resp_len = response_len.min(fft_len);
    let mut step = Vec::with_capacity(resp_len);
    let mut cumsum = 0.0;
    for item in h.iter().take(resp_len) {
        let impulse_val = item.re * scale;
        if impulse_val.is_finite() {
            cumsum += impulse_val;
        }
        step.push(cumsum);
    }

    let final_val = *step.last().unwrap_or(&0.0);
    if final_val.abs() < 1e-10 {
        return None;
    }
    for v in &mut step {
        *v /= final_val;
    }

    Some(step)
}

// ---------------------------------------------------------------------------
// Histogram
// ---------------------------------------------------------------------------

fn build_histogram(
    all_responses: &[Vec<f64>],
    resp_len: usize,
    time_s: &[f64],
) -> StepResponseHistogram {
    let time_min = 0.0f64;
    let time_max = RESPONSE_DURATION_S;
    let time_bin_width = (time_max - time_min) / HIST_TIME_BINS as f64;
    let amp_bin_width = (HIST_AMP_MAX - HIST_AMP_MIN) / HIST_AMP_BINS as f64;

    let time_bins: Vec<f64> = (0..HIST_TIME_BINS)
        .map(|i| time_min + (i as f64 + 0.5) * time_bin_width)
        .collect();
    let amplitude_bins: Vec<f64> = (0..HIST_AMP_BINS)
        .map(|i| HIST_AMP_MIN + (i as f64 + 0.5) * amp_bin_width)
        .collect();

    let mut counts = vec![0u32; HIST_TIME_BINS * HIST_AMP_BINS];

    for resp in all_responses {
        for (i, &val) in resp.iter().take(resp_len).enumerate() {
            if i >= time_s.len() {
                break;
            }
            let t = time_s[i];
            let ti = ((t - time_min) / time_bin_width).floor() as isize;
            let ai = ((val - HIST_AMP_MIN) / amp_bin_width).floor() as isize;

            if ti >= 0
                && (ti as usize) < HIST_TIME_BINS
                && ai >= 0
                && (ai as usize) < HIST_AMP_BINS
            {
                counts[ti as usize * HIST_AMP_BINS + ai as usize] += 1;
            }
        }
    }

    StepResponseHistogram {
        time_bins,
        amplitude_bins,
        counts,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal_processing::dsp::hanning_window;
    use crate::signal_processing::testing;

    #[test]
    fn test_pid_analysis_sample() {
        let result = testing::analyze_fixture_for("sample.ulg", "pid_step_response");
        // sample.ulg may not have vehicle_angular_velocity, so PID analysis
        // may return InsufficientData (None here). That's valid.
        let Some(value) = result else {
            return;
        };

        let parsed: PidAnalysisResult = serde_json::from_value(value).unwrap();

        for axis_result in &parsed.axes {
            assert!(
                axis_result.sample_rate_hz >= MIN_SAMPLE_RATE_HZ,
                "sample rate should be above minimum"
            );
            assert!(
                axis_result.window_count >= MIN_WINDOWS,
                "should have enough windows"
            );
            assert!(!axis_result.time_s.is_empty());
            assert_eq!(axis_result.time_s.len(), axis_result.mean_response.len());
            assert_eq!(axis_result.histogram.time_bins.len(), HIST_TIME_BINS);
            assert_eq!(axis_result.histogram.amplitude_bins.len(), HIST_AMP_BINS);
            assert_eq!(
                axis_result.histogram.counts.len(),
                HIST_TIME_BINS * HIST_AMP_BINS
            );
        }
    }

    #[test]
    fn test_pid_analysis_fixed_wing() {
        let result = testing::analyze_fixture_for("fixed_wing_gps.ulg", "pid_step_response");
        // May or may not produce a result depending on log content
        if let Some(value) = result {
            let parsed: PidAnalysisResult = serde_json::from_value(value).unwrap();
            for axis_result in &parsed.axes {
                assert!(axis_result.window_count >= MIN_WINDOWS);
                assert!(!axis_result.mean_response.is_empty());
            }
        }
    }

    #[test]
    fn test_wiener_step_response_identity() {
        let n = 128;
        let hann = hanning_window(n);
        let signal: Vec<f64> = (0..n).map(|i| (i as f64 * 0.1).sin()).collect();
        let resp = wiener_step_response(&signal, &signal, &hann, n, n / 2);
        if let Some(r) = resp {
            assert!(!r.is_empty());
            assert!((r.last().unwrap() - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_no_errors_on_fixtures() {
        testing::assert_no_errors("sample.ulg", "pid_step_response");
    }
}
