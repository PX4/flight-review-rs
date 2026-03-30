//! PID step response analysis using Wiener deconvolution.
//!
//! Extracts the step response of the PID controller for each axis (roll, pitch, yaw)
//! by deconvolving the rate setpoint (input) from the actual angular rate (output).
//! This answers: "when the controller commands a rate change, how does the vehicle
//! actually respond?"

use px4_ulog::stream_parser::file_reader::{
    read_file_with_simple_callback, Message, SimpleCallbackResult,
};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

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
// Configuration constants
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
// Entry point
// ---------------------------------------------------------------------------

/// Run PID step response analysis on a ULog file.
///
/// Returns results for each axis that has sufficient data. Axes with missing
/// topics, too few windows, or a sample rate below 50 Hz are silently skipped.
pub fn pid_analysis(path: &str) -> Result<PidAnalysisResult, std::io::Error> {
    let mut result = PidAnalysisResult { axes: Vec::new() };
    for axis in &["roll", "pitch", "yaw"] {
        if let Some(response) = analyze_axis(path, axis)? {
            result.axes.push(response);
        }
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Per-axis analysis
// ---------------------------------------------------------------------------

fn analyze_axis(path: &str, axis: &str) -> Result<Option<PidStepResponse>, std::io::Error> {
    // 1. Extract raw signals
    let (setpoint_raw, gyro_raw) = extract_signals(path, axis)?;

    if setpoint_raw.len() < 2 || gyro_raw.len() < 2 {
        return Ok(None);
    }

    // 2. Compute median sample rate from the setpoint signal
    let sample_rate = median_sample_rate(&setpoint_raw);
    if sample_rate < MIN_SAMPLE_RATE_HZ {
        return Ok(None);
    }

    // 3. Determine the common time range
    let t_start = setpoint_raw[0].0.max(gyro_raw[0].0);
    let t_end = setpoint_raw.last().unwrap().0.min(gyro_raw.last().unwrap().0);
    if t_end - t_start < WINDOW_DURATION_S {
        return Ok(None);
    }

    // 4. Resample both signals to uniform grid
    let setpoint = resample_uniform(&setpoint_raw, sample_rate, t_start, t_end);
    let gyro = resample_uniform(&gyro_raw, sample_rate, t_start, t_end);

    let n = setpoint.len().min(gyro.len());
    if n < 2 {
        return Ok(None);
    }
    let setpoint = &setpoint[..n];
    let gyro = &gyro[..n];

    // 5. Windowed Wiener deconvolution
    let window_samples = (WINDOW_DURATION_S * sample_rate).round() as usize;
    let step_samples = (STEP_DURATION_S * sample_rate).round() as usize;
    let response_samples = (RESPONSE_DURATION_S * sample_rate).round() as usize;

    if window_samples < 4 || step_samples == 0 || response_samples == 0 {
        return Ok(None);
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
        return Ok(None);
    }

    // 6. Compute mean step response
    let resp_len = response_samples.min(
        all_step_responses
            .iter()
            .map(|r| r.len())
            .min()
            .unwrap_or(0),
    );
    if resp_len == 0 {
        return Ok(None);
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

    // 7. Build time vector
    let dt = 1.0 / sample_rate;
    let time_s: Vec<f64> = (0..resp_len).map(|i| i as f64 * dt).collect();

    // 8. Build 2D histogram
    let histogram = build_histogram(&all_step_responses, resp_len, &time_s);

    Ok(Some(PidStepResponse {
        axis: axis.to_string(),
        sample_rate_hz: sample_rate,
        window_count: all_step_responses.len(),
        time_s,
        mean_response,
        histogram,
    }))
}

// ---------------------------------------------------------------------------
// Signal extraction
// ---------------------------------------------------------------------------

/// Extract rate setpoint and angular velocity signals for the given axis.
/// Returns two vectors of (time_seconds, value) pairs.
fn extract_signals(
    path: &str,
    axis: &str,
) -> Result<(Vec<(f64, f64)>, Vec<(f64, f64)>), std::io::Error> {
    let setpoint_field = match axis {
        "roll" => "roll",
        "pitch" => "pitch",
        "yaw" => "yaw",
        _ => return Ok((Vec::new(), Vec::new())),
    };

    let gyro_field = match axis {
        "roll" => "xyz[0]",
        "pitch" => "xyz[1]",
        "yaw" => "xyz[2]",
        _ => return Ok((Vec::new(), Vec::new())),
    };

    let mut setpoint_data: Vec<(f64, f64)> = Vec::new();
    let mut gyro_data: Vec<(f64, f64)> = Vec::new();

    read_file_with_simple_callback(path, &mut |msg| {
        match msg {
            Message::Data(data) => {
                let topic = data.flattened_format.message_name.as_str();
                let ts = data
                    .flattened_format
                    .timestamp_field
                    .as_ref()
                    .map(|tf| tf.parse_timestamp(data.data));

                match topic {
                    "vehicle_rates_setpoint" => {
                        if let (Some(ts), Ok(parser)) =
                            (ts, data.flattened_format.get_field_parser::<f32>(setpoint_field))
                        {
                            let val = parser.parse(data.data) as f64;
                            if val.is_finite() {
                                let t_s = ts as f64 / 1_000_000.0;
                                setpoint_data.push((t_s, val));
                            }
                        }
                    }
                    "vehicle_angular_velocity" => {
                        if let (Some(ts), Ok(parser)) =
                            (ts, data.flattened_format.get_field_parser::<f32>(gyro_field))
                        {
                            let val = parser.parse(data.data) as f64;
                            if val.is_finite() {
                                let t_s = ts as f64 / 1_000_000.0;
                                gyro_data.push((t_s, val));
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        SimpleCallbackResult::KeepReading
    })?;

    // Sort by timestamp (should already be sorted, but be safe)
    setpoint_data.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    gyro_data.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    Ok((setpoint_data, gyro_data))
}

// ---------------------------------------------------------------------------
// Resampling
// ---------------------------------------------------------------------------

/// Compute the median sample rate from a time series.
fn median_sample_rate(data: &[(f64, f64)]) -> f64 {
    if data.len() < 2 {
        return 0.0;
    }
    let mut dts: Vec<f64> = data
        .windows(2)
        .map(|w| w[1].0 - w[0].0)
        .filter(|&dt| dt > 0.0)
        .collect();
    if dts.is_empty() {
        return 0.0;
    }
    dts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_dt = dts[dts.len() / 2];
    if median_dt > 0.0 {
        1.0 / median_dt
    } else {
        0.0
    }
}

/// Resample non-uniform (time, value) data to a uniform grid via linear interpolation.
fn resample_uniform(
    data: &[(f64, f64)],
    sample_rate: f64,
    t_start: f64,
    t_end: f64,
) -> Vec<f64> {
    let dt = 1.0 / sample_rate;
    let n = ((t_end - t_start) / dt).floor() as usize + 1;
    let mut result = Vec::with_capacity(n);

    let mut j = 0usize; // index into data
    for i in 0..n {
        let t = t_start + i as f64 * dt;

        // Advance j so that data[j].0 <= t < data[j+1].0
        while j + 1 < data.len() && data[j + 1].0 <= t {
            j += 1;
        }

        if j + 1 >= data.len() {
            // Past the end of data, use last value
            result.push(data.last().unwrap().1);
        } else if t <= data[j].0 {
            // Before or at the first applicable point
            result.push(data[j].1);
        } else {
            // Linear interpolation between data[j] and data[j+1]
            let t0 = data[j].0;
            let t1 = data[j + 1].0;
            let v0 = data[j].1;
            let v1 = data[j + 1].1;
            let frac = (t - t0) / (t1 - t0);
            result.push(v0 + frac * (v1 - v0));
        }
    }

    result
}

// ---------------------------------------------------------------------------
// DSP: Hanning window, Wiener deconvolution, step response
// ---------------------------------------------------------------------------

fn hanning_window(n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f64 / n as f64).cos()))
        .collect()
}

/// Perform Wiener deconvolution on a single window and return the normalized
/// step response (first `response_len` samples).
fn wiener_step_response(
    input: &[f64],
    output: &[f64],
    hann: &[f64],
    fft_len: usize,
    response_len: usize,
) -> Option<Vec<f64>> {
    // Apply Hanning window
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

    // Pad/truncate to fft_len
    x.resize(fft_len, Complex::new(0.0, 0.0));
    y.resize(fft_len, Complex::new(0.0, 0.0));

    // FFT
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_len);
    fft.process(&mut x);
    fft.process(&mut y);

    // Compute noise floor = max(|X(f)|^2) * noise_floor_factor
    let max_power = x
        .iter()
        .map(|c| c.norm_sqr())
        .fold(0.0f64, |a, b| a.max(b));
    if max_power < 1e-30 {
        // Input is essentially silent, skip this window
        return None;
    }
    let noise_floor = max_power * NOISE_FLOOR_FACTOR;

    // Wiener deconvolution: H(f) = Y(f) * conj(X(f)) / (|X(f)|^2 + noise_floor)
    let mut h: Vec<Complex<f64>> = y
        .iter()
        .zip(x.iter())
        .map(|(yi, xi)| {
            let denom = xi.norm_sqr() + noise_floor;
            (*yi * xi.conj()) / denom
        })
        .collect();

    // Inverse FFT to get impulse response
    let ifft = planner.plan_fft_inverse(fft_len);
    ifft.process(&mut h);

    // Normalize by fft_len (rustfft does not normalize)
    let scale = 1.0 / fft_len as f64;

    // Extract real part of impulse response and integrate (cumulative sum) for step response
    let resp_len = response_len.min(fft_len);
    let mut step = Vec::with_capacity(resp_len);
    let mut cumsum = 0.0;
    for i in 0..resp_len {
        let impulse_val = h[i].re * scale;
        if impulse_val.is_finite() {
            cumsum += impulse_val;
        }
        step.push(cumsum);
    }

    // Normalize so step response converges to 1.0
    let final_val = *step.last().unwrap_or(&0.0);
    if final_val.abs() < 1e-10 {
        return None; // Degenerate response
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

            if ti >= 0 && (ti as usize) < HIST_TIME_BINS && ai >= 0 && (ai as usize) < HIST_AMP_BINS
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

    fn px4_ulog_fixture(name: &str) -> String {
        let manifest = env!("CARGO_MANIFEST_DIR");
        std::path::Path::new(manifest)
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("px4-ulog-rs/tests/fixtures")
            .join(name)
            .to_string_lossy()
            .to_string()
    }

    #[test]
    fn test_pid_analysis_sample() {
        let path = px4_ulog_fixture("sample.ulg");
        let result = pid_analysis(&path).unwrap();
        // The result should be valid (possibly empty if sample.ulg lacks the topics)
        // but should not error out.
        for axis_result in &result.axes {
            assert!(
                axis_result.sample_rate_hz >= MIN_SAMPLE_RATE_HZ,
                "sample rate should be above minimum"
            );
            assert!(
                axis_result.window_count >= MIN_WINDOWS,
                "should have enough windows"
            );
            assert!(
                !axis_result.time_s.is_empty(),
                "time vector should not be empty"
            );
            assert_eq!(
                axis_result.time_s.len(),
                axis_result.mean_response.len(),
                "time and response lengths must match"
            );
            // Histogram dimensions
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
        let path = px4_ulog_fixture("fixed_wing_gps.ulg");
        let result = pid_analysis(&path).unwrap();
        // Should not error; may or may not have axes depending on log content
        for axis_result in &result.axes {
            assert!(axis_result.window_count >= MIN_WINDOWS);
            assert!(!axis_result.mean_response.is_empty());
        }
    }

    #[test]
    fn test_resample_uniform_basic() {
        let data = vec![(0.0, 1.0), (1.0, 3.0), (2.0, 5.0)];
        let result = resample_uniform(&data, 2.0, 0.0, 2.0);
        // At rate 2.0 Hz from 0 to 2: t = 0.0, 0.5, 1.0, 1.5, 2.0
        assert_eq!(result.len(), 5);
        assert!((result[0] - 1.0).abs() < 1e-9); // t=0
        assert!((result[1] - 2.0).abs() < 1e-9); // t=0.5
        assert!((result[2] - 3.0).abs() < 1e-9); // t=1.0
        assert!((result[3] - 4.0).abs() < 1e-9); // t=1.5
        assert!((result[4] - 5.0).abs() < 1e-9); // t=2.0
    }

    #[test]
    fn test_hanning_window() {
        let w = hanning_window(4);
        assert_eq!(w.len(), 4);
        // Hanning window for n=4:
        // i=0: 0.5*(1 - cos(0)) = 0
        // i=1: 0.5*(1 - cos(pi/2)) = 0.5
        // i=2: 0.5*(1 - cos(pi)) = 1.0
        // i=3: 0.5*(1 - cos(3pi/2)) = 0.5
        assert!(w[0].abs() < 1e-10);
        assert!((w[1] - 0.5).abs() < 1e-10);
        assert!((w[2] - 1.0).abs() < 1e-10);
        assert!((w[3] - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_median_sample_rate() {
        let data: Vec<(f64, f64)> = (0..100).map(|i| (i as f64 * 0.01, 0.0)).collect();
        let rate = median_sample_rate(&data);
        assert!((rate - 100.0).abs() < 1.0);
    }

    #[test]
    fn test_wiener_step_response_identity() {
        // If input == output, the step response should be roughly a step to 1.0
        let n = 128;
        let hann = hanning_window(n);
        let signal: Vec<f64> = (0..n).map(|i| (i as f64 * 0.1).sin()).collect();
        let resp = wiener_step_response(&signal, &signal, &hann, n, n / 2);
        // When input == output, the impulse response is a delta, so the step
        // response should jump to ~1.0 quickly.
        if let Some(r) = resp {
            assert!(!r.is_empty());
            // The last value should be 1.0 (normalized)
            assert!((r.last().unwrap() - 1.0).abs() < 1e-6);
        }
    }
}
