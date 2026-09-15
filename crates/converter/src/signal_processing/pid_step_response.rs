//! PID step response analysis using Wiener deconvolution.
//!
//! Extracts the step response of the PID controller for each axis (roll,
//! pitch, yaw) by deconvolving the rate setpoint (input) from the actual
//! angular rate (output).
//!
//! Only broadband-excited, sufficiently sampled windows with predominantly
//! causal, settling estimates are retained. These conservative heuristics can
//! reject otherwise useful band-limited flights; they are not confidence scores
//! or a substitute for a dedicated system-identification experiment.

use super::dsp::{hanning_window, median_sample_rate, resample_covered_window};
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
    /// Estimated output/input step gain, without endpoint normalization.
    /// Screening is heuristic, not a calibrated confidence measure.
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
// A transfer curve is not identifiable from a constant or a few excited bins.
// Require substantial excitation across the sampled bandwidth. These are
// conservative screening thresholds, not statistical confidence bounds.
const MIN_EXCITED_BIN_FRACTION: f64 = 0.25;
const EXCITED_POWER_FRACTION: f64 = 0.01;
const MAX_NONCAUSAL_ENERGY_FRACTION: f64 = 0.2;

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
        let mut unavailable = Vec::new();

        for (axis_name, setpoint_field, gyro_field) in AXES {
            let setpoint_req = SignalRequest::new("vehicle_rates_setpoint", setpoint_field);
            let gyro_req = SignalRequest::new("vehicle_angular_velocity", gyro_field);

            let setpoint_raw = signals.get(&setpoint_req);
            let gyro_raw = signals.get(&gyro_req);

            match analyze_axis(axis_name, setpoint_raw, gyro_raw) {
                Ok(response) => result.axes.push(response),
                Err(reason) => unavailable.push(format!("{axis_name}: {reason}")),
            }
        }

        if result.axes.is_empty() {
            return Err(AnalysisError::InsufficientData {
                reason: format!(
                    "No axis met PID step-response criteria. {}",
                    unavailable.join("; ")
                ),
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
) -> Result<PidStepResponse, String> {
    if setpoint_raw.len() < 2 || gyro_raw.len() < 2 {
        return Err(format!(
            "not enough data: {} rate-setpoint samples and {} measured-rate samples; need at least 2 of each",
            setpoint_raw.len(), gyro_raw.len()
        ));
    }

    let setpoint_rate = median_sample_rate(setpoint_raw);
    let gyro_rate = median_sample_rate(gyro_raw);
    let sample_rate = setpoint_rate.min(gyro_rate);
    if !sample_rate.is_finite() || sample_rate < MIN_SAMPLE_RATE_HZ * (1.0 - 1e-9) {
        return Err(format!(
            "insufficient sampling or invalid timestamps: setpoint {setpoint_rate:.1} Hz, measured rate {gyro_rate:.1} Hz; both need at least {MIN_SAMPLE_RATE_HZ:.0} Hz"
        ));
    }

    let t_start = setpoint_raw[0].0.max(gyro_raw[0].0);
    let t_end = setpoint_raw
        .last()
        .unwrap()
        .0
        .min(gyro_raw.last().unwrap().0);
    if t_end - t_start < WINDOW_DURATION_S {
        return Err(format!(
            "insufficient overlapping data: {:.2} s; need at least {WINDOW_DURATION_S:.1} s",
            (t_end - t_start).max(0.0)
        ));
    }

    let window_samples = (WINDOW_DURATION_S * sample_rate).round() as usize;
    let step_samples = (STEP_DURATION_S * sample_rate).round() as usize;
    let response_samples = (RESPONSE_DURATION_S * sample_rate).round() as usize;

    if window_samples < 4 || step_samples == 0 || response_samples == 0 {
        return Err("sampling cannot form a usable analysis window".into());
    }

    let hann = hanning_window(window_samples);
    let mut all_step_responses: Vec<Vec<f64>> = Vec::new();
    let mut attempted_windows = 0;
    let mut covered_windows = 0;

    let mut offset = 0;
    while t_start + (offset + window_samples - 1) as f64 / sample_rate <= t_end {
        attempted_windows += 1;
        let start = t_start + offset as f64 / sample_rate;
        if let (Some(sp_win), Some(gy_win)) = (
            resample_covered_window(
                setpoint_raw,
                setpoint_rate,
                sample_rate,
                start,
                window_samples,
            ),
            resample_covered_window(gyro_raw, gyro_rate, sample_rate, start, window_samples),
        ) {
            covered_windows += 1;
            if let Some(step) =
                wiener_step_response(&sp_win, &gy_win, &hann, window_samples, response_samples)
            {
                all_step_responses.push(step);
            }
        }

        offset += step_samples;
    }

    if all_step_responses.len() < MIN_WINDOWS {
        return Err(if attempted_windows < MIN_WINDOWS {
            format!(
                "only {attempted_windows} overlapping analysis windows; need at least {MIN_WINDOWS}"
            )
        } else if covered_windows < MIN_WINDOWS {
            format!(
                "only {covered_windows} windows have contiguous finite setpoint and measured-rate data; need at least {MIN_WINDOWS}; gaps or unavailable values prevent analysis"
            )
        } else {
            format!(
                "only {} of {covered_windows} windows meet excitation and response-quality criteria; need at least {MIN_WINDOWS}; insufficient excitation or noncausal/unsettled responses cannot produce a useful estimate",
                all_step_responses.len()
            )
        });
    }

    let resp_len = response_samples.min(
        all_step_responses
            .iter()
            .map(|r| r.len())
            .min()
            .unwrap_or(0),
    );
    if resp_len == 0 {
        return Err("qualified windows contained no response samples".into());
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

    Ok(PidStepResponse {
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
    if fft_len < 4
        || input.len() != fft_len
        || output.len() != fft_len
        || hann.len() != fft_len
        || response_len == 0
        || input
            .iter()
            .chain(output)
            .chain(hann)
            .any(|v| !v.is_finite())
    {
        return None;
    }
    let input_mean = input.iter().sum::<f64>() / input.len() as f64;
    let output_mean = output.iter().sum::<f64>() / output.len() as f64;
    let mut x: Vec<Complex<f64>> = input
        .iter()
        .zip(hann.iter())
        .map(|(&s, &w)| Complex::new((s - input_mean) * w, 0.0))
        .collect();
    let mut y: Vec<Complex<f64>> = output
        .iter()
        .zip(hann.iter())
        .map(|(&s, &w)| Complex::new((s - output_mean) * w, 0.0))
        .collect();

    x.resize(fft_len, Complex::new(0.0, 0.0));
    y.resize(fft_len, Complex::new(0.0, 0.0));

    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_len);
    fft.process(&mut x);
    fft.process(&mut y);

    let max_power = x.iter().map(|c| c.norm_sqr()).fold(0.0f64, |a, b| a.max(b));
    if max_power < 1e-30 {
        return None;
    }
    let noise_floor = max_power * NOISE_FLOOR_FACTOR;

    let positive_power: Vec<f64> = x[1..=fft_len / 2].iter().map(|c| c.norm_sqr()).collect();
    let excited = |p: &&f64| **p >= max_power * EXCITED_POWER_FRACTION;
    // The low-frequency gain is essential to a step curve; broadband power
    // elsewhere cannot compensate for a DC estimate dominated by regularization.
    if x[0].norm_sqr() < max_power * EXCITED_POWER_FRACTION
        || (positive_power.iter().filter(excited).count() as f64)
            < positive_power.len() as f64 * MIN_EXCITED_BIN_FRACTION
        || positive_power
            .chunks(positive_power.len().div_ceil(4))
            .any(|band| {
                !band
                    .iter()
                    .any(|p| *p >= max_power * EXCITED_POWER_FRACTION)
            })
    {
        return None;
    }

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
    let total_energy = h.iter().map(|v| v.norm_sqr()).sum::<f64>();
    let noncausal_energy = h[fft_len / 2..].iter().map(|v| v.norm_sqr()).sum::<f64>();
    if !total_energy.is_finite()
        || total_energy < 1e-30
        || noncausal_energy > total_energy * MAX_NONCAUSAL_ENERGY_FRACTION
    {
        return None;
    }

    let resp_len = response_len.min(fft_len);
    let mut step = Vec::with_capacity(resp_len);
    let mut cumsum = 0.0;
    for item in h.iter().take(resp_len) {
        let impulse_val = item.re * scale;
        if !impulse_val.is_finite() {
            return None;
        }
        cumsum += impulse_val;
        step.push(cumsum);
    }

    let peak = step.iter().map(|v| v.abs()).fold(0.0_f64, f64::max);
    let tail = &step[step.len() * 4 / 5..];
    let tail_min = tail.iter().copied().fold(f64::INFINITY, f64::min);
    let tail_max = tail.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if !peak.is_finite() || peak < 1e-10 || tail_max - tail_min > 0.2 * peak {
        return None;
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
    use crate::signal_processing::dsp::hanning_window;
    use crate::signal_processing::testing;

    #[test]
    fn sample_without_modern_gyro_is_explicitly_insufficient() {
        assert!(testing::analyze_fixture_for("sample.ulg", "pid_step_response").is_none());
    }

    #[test]
    fn narrowband_and_constant_are_not_identifiable() {
        let n = 128;
        for signal in [
            vec![1.0; n],
            vec![0.0; n],
            (0..n).map(|i| (i as f64 * 0.1).sin()).collect(),
            (0..n).map(|i| 10.0 + (i as f64 * 0.1).sin()).collect(),
        ] {
            assert!(wiener_step_response(&signal, &signal, &hanning_window(n), n, n / 2).is_none());
        }
    }

    #[test]
    fn unavailable_reasons_distinguish_sampling_coverage_and_quality() {
        assert!(analyze_axis("roll", &[], &[])
            .unwrap_err()
            .contains("not enough data"));
        let slow: Vec<_> = (0..10).map(|i| (i as f64, 1.0)).collect();
        assert!(analyze_axis("roll", &slow, &slow)
            .unwrap_err()
            .contains("both need at least 50 Hz"));
        let short: Vec<_> = (0..40).map(|i| (i as f64 / 100.0, 1.0)).collect();
        assert!(analyze_axis("roll", &short, &short)
            .unwrap_err()
            .contains("overlapping data"));
        let constant: Vec<_> = (0..301).map(|i| (i as f64 / 100.0, 1.0)).collect();
        let missing: Vec<_> = constant.iter().map(|(t, _)| (*t, f64::NAN)).collect();
        assert!(analyze_axis("roll", &constant, &missing)
            .unwrap_err()
            .contains("contiguous finite"));
        assert!(analyze_axis("roll", &constant, &constant)
            .unwrap_err()
            .contains("excitation and response-quality criteria"));
    }

    #[test]
    fn broadband_identity_preserves_the_entire_gain_curve() {
        let n = 128;
        let signal = testing::broadband(n);
        for gain in [0.5, 1.0, 2.0] {
            let output: Vec<_> = signal.iter().map(|v| gain * v).collect();
            let response = wiener_step_response(&signal, &output, &hanning_window(n), n, n / 2)
                .expect("broadband identity must be identifiable");
            assert_eq!(response.len(), n / 2);
            for (i, value) in response.iter().enumerate() {
                assert!(
                    (value - gain).abs() < 0.08 * gain,
                    "sample {i}: {value}, gain {gain}"
                );
            }
        }
    }

    #[test]
    fn broadband_without_low_frequency_gain_is_not_identifiable() {
        let n = 128;
        let hann = hanning_window(n);
        let mut signal = testing::broadband(n);
        let centered_hann: Vec<_> = hann.iter().map(|w| w - 0.5).collect();
        let projection = signal
            .iter()
            .zip(&centered_hann)
            .map(|(x, w)| x * w)
            .sum::<f64>()
            / centered_hann.iter().map(|w| w * w).sum::<f64>();
        for (x, w) in signal.iter_mut().zip(centered_hann) {
            *x -= projection * w;
        }
        assert!(wiener_step_response(&signal, &signal, &hann, n, n / 2).is_none());
    }

    fn samples(signal: &[f64], rate: usize) -> Vec<(u64, [f32; 3])> {
        signal
            .iter()
            .enumerate()
            .map(|(i, value)| {
                (
                    1_000_000 + i as u64 * 1_000_000 / rate as u64,
                    [*value as f32; 3],
                )
            })
            .collect()
    }

    fn first_order(input: &[f64], rate: f64) -> Vec<f64> {
        let pole = (-1.0 / (rate * 0.05)).exp();
        let mut state = 0.0;
        input
            .iter()
            .map(|value| {
                state = pole * state + (1.0 - pole) * value;
                state
            })
            .collect()
    }

    #[test]
    fn modern_ulog_identity_and_50ms_first_order_full_curves() {
        for rate in [50, 100, 250] {
            let input = testing::broadband(rate * 12);
            for filtered in [false, true] {
                let output = if filtered {
                    first_order(&input, rate as f64)
                } else {
                    input.clone()
                };
                let log =
                    testing::PidLog::new(&samples(&input, rate), &samples(&output, rate), &[]);
                let value = log
                    .analyze()
                    .remove("pid_step_response")
                    .expect("known system must produce PID output");
                let result: PidAnalysisResult = serde_json::from_value(value).unwrap();
                assert_eq!(result.axes.len(), 3);
                for axis in result.axes {
                    assert!(axis.window_count >= 10, "{}", axis.window_count);
                    assert!((axis.sample_rate_hz - rate as f64).abs() < 0.01);
                    assert_eq!(axis.time_s.len(), rate / 2);
                    assert_eq!(axis.mean_response.len(), axis.time_s.len());
                    let mut squared_error = 0.0;
                    for (i, (&time, &value)) in
                        axis.time_s.iter().zip(&axis.mean_response).enumerate()
                    {
                        let expected = if filtered {
                            1.0 - (-(time + 1.0 / rate as f64) / 0.05).exp()
                        } else {
                            1.0
                        };
                        squared_error += (value - expected).powi(2);
                        let tolerance = if filtered { 0.12 } else { 0.05 };
                        assert!((value - expected).abs() < tolerance,
                            "rate {rate}, filtered {filtered}, sample {i}: {value}, expected {expected}");
                    }
                    let rmse = (squared_error / axis.time_s.len() as f64).sqrt();
                    assert!(rmse < 0.08, "rate {rate}, filtered {filtered}, RMSE {rmse}");
                    assert_eq!(axis.histogram.time_bins.len(), HIST_TIME_BINS);
                    assert_eq!(axis.histogram.amplitude_bins.len(), HIST_AMP_BINS);
                    assert_eq!(axis.histogram.counts.len(), HIST_TIME_BINS * HIST_AMP_BINS);
                    assert!(axis.histogram.counts.iter().sum::<u32>() > 0);
                }
            }
        }
    }

    #[test]
    fn both_streams_require_rate_order_and_coverage() {
        let good = samples(&testing::broadband(301), 100);
        let mut backwards = good.clone();
        backwards.swap(50, 51);
        let mut conflicting_duplicate = good.clone();
        conflicting_duplicate[51].0 = conflicting_duplicate[50].0;
        let mut missing = good.clone();
        for sample in &mut missing[50..251] {
            sample.1 = [f32::NAN; 3];
        }
        let bad_streams = [
            Vec::new(),
            vec![good[0], *good.last().unwrap()],
            samples(&vec![0.0; good.len()], 100),
            good.iter().step_by(5).copied().collect(),
            good.iter()
                .copied()
                .filter(|p| p.0 < 1_500_000 || p.0 > 3_500_000)
                .collect(),
            backwards,
            conflicting_duplicate,
            missing,
        ];
        for (case, bad) in bad_streams.iter().enumerate() {
            for (sp, gy) in [(bad, &good), (&good, bad)] {
                assert!(
                    testing::PidLog::new(sp, gy, &[]).analyze().is_empty(),
                    "case {case}"
                );
            }
        }
    }

    #[test]
    fn gaps_discard_only_affected_windows() {
        let input = testing::broadband(1001);
        let good = samples(&input, 100);
        let gapped: Vec<_> = good
            .iter()
            .copied()
            .filter(|p| p.0 < 5_000_000 || p.0 > 7_000_000)
            .collect();
        let baseline: PidAnalysisResult = serde_json::from_value(
            testing::PidLog::new(&good, &good, &[])
                .analyze()
                .remove("pid_step_response")
                .unwrap(),
        )
        .unwrap();
        for (sp, gy) in [(&good, &gapped), (&gapped, &good)] {
            let result: PidAnalysisResult = serde_json::from_value(
                testing::PidLog::new(sp, gy, &[])
                    .analyze()
                    .remove("pid_step_response")
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(result.axes.len(), 3);
            for axis in result.axes {
                assert!(
                    axis.window_count >= MIN_WINDOWS
                        && axis.window_count <= 14
                        && axis.window_count < baseline.axes[0].window_count,
                    "{}",
                    axis.window_count
                );
                assert!(axis.mean_response.iter().all(|v| (v - 1.0).abs() < 0.12));
            }
        }
    }

    #[test]
    fn extraction_selects_instances_and_collapses_only_identical_duplicates() {
        let good = samples(&testing::broadband(501), 100);
        let duplicate: Vec<_> = good.iter().flat_map(|p| [*p, *p]).collect();
        let other = samples(&vec![99.0; good.len()], 100);
        let log = testing::PidLog::new(&duplicate, &duplicate, &other);
        let default = SignalRequest::new("vehicle_angular_velocity", "xyz[0]");
        let secondary = default.clone().with_instance(1);
        let missing = SignalRequest::new("vehicle_angular_velocity", "missing_field");
        let store = crate::signal_processing::extract_signals(
            log.path.to_str().unwrap(),
            &[default.clone(), secondary.clone(), missing.clone()]
                .into_iter()
                .collect(),
        )
        .unwrap();
        assert_eq!(store.get(&default).len(), good.len());
        assert_eq!(store.get(&secondary).len(), other.len());
        assert!(store.get(&secondary).iter().all(|p| p.1 == 99.0));
        assert_eq!(store.get(&missing).len(), good.len());
        assert!(store.get(&missing).iter().all(|p| p.1.is_nan()));
        assert_eq!(log.analyze().len(), 1);
        assert!(testing::PidLog::new(&good, &[], &good).analyze().is_empty());
    }

    #[test]
    fn unequal_source_rates_use_the_slower_qualified_grid() {
        let signal = testing::broadband(601);
        let coarse = samples(&signal, 100);
        let interpolated: Vec<_> = signal
            .windows(2)
            .flat_map(|pair| [pair[0], (pair[0] + pair[1]) / 2.0])
            .collect();
        let fine = samples(&interpolated, 200);
        for (sp, gy) in [(&coarse, &fine), (&fine, &coarse)] {
            let result: PidAnalysisResult = serde_json::from_value(
                testing::PidLog::new(sp, gy, &[])
                    .analyze()
                    .remove("pid_step_response")
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(result.axes.len(), 3);
            for axis in result.axes {
                assert!((axis.sample_rate_hz - 100.0).abs() < 0.01);
                assert!(axis.mean_response.iter().all(|v| (v - 1.0).abs() < 0.05));
            }
        }
    }

    #[test]
    fn pipeline_rejects_constant_and_narrowband_inputs() {
        for signal in [
            vec![1.0; 501],
            (0..501).map(|i| (i as f64 * 0.1).sin()).collect(),
        ] {
            let stream = samples(&signal, 100);
            assert!(testing::PidLog::new(&stream, &stream, &[])
                .analyze()
                .is_empty());
        }
    }

    #[test]
    fn noncausal_and_unrelated_outputs_are_rejected() {
        let n = 256;
        let signal = testing::broadband(n * 2);
        let advanced: Vec<_> = (0..n).map(|i| signal[(i + 20) % n]).collect();
        for output in [&advanced[..], &signal[n..]] {
            assert!(
                wiener_step_response(&signal[..n], output, &hanning_window(n), n, n / 2).is_none()
            );
        }
    }

    #[test]
    fn malformed_fft_inputs_are_rejected() {
        let good = testing::broadband(128);
        let mut nonfinite = good.clone();
        nonfinite[10] = f64::NAN;
        let hann = hanning_window(128);
        for (input, output, window, n, response_len) in [
            (&good[..127], &good[..], &hann[..], 128, 64),
            (&good[..], &good[..127], &hann[..], 128, 64),
            (&good[..], &good[..], &hann[..127], 128, 64),
            (&nonfinite[..], &good[..], &hann[..], 128, 64),
            (&good[..], &nonfinite[..], &hann[..], 128, 64),
            (&good[..], &good[..], &hann[..], 128, 0),
        ] {
            assert!(wiener_step_response(input, output, window, n, response_len).is_none());
        }
    }

    #[test]
    fn test_no_errors_on_fixtures() {
        testing::assert_no_errors("sample.ulg", "pid_step_response");
    }
}
