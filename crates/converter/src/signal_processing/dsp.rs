//! Shared DSP utilities for signal processing modules.
//!
//! Provides common functions for resampling, windowing, and sample rate
//! estimation. These are used by multiple analysis modules.

use std::f64::consts::PI;

/// Compute the median sample rate from a non-uniform time series.
///
/// Returns 0.0 if the time series has fewer than 2 points or all
/// intervals are zero.
pub fn median_sample_rate(data: &[(f64, f64)]) -> f64 {
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
///
/// Creates a uniform time grid from `t_start` to `t_end` at the given
/// `sample_rate` (Hz) and interpolates values linearly between adjacent
/// input samples.
pub fn resample_uniform(
    data: &[(f64, f64)],
    sample_rate: f64,
    t_start: f64,
    t_end: f64,
) -> Vec<f64> {
    let dt = 1.0 / sample_rate;
    let n = ((t_end - t_start) / dt).floor() as usize + 1;
    let mut result = Vec::with_capacity(n);

    let mut j = 0usize;
    for i in 0..n {
        let t = t_start + i as f64 * dt;

        // Advance j so that data[j].0 <= t < data[j+1].0
        while j + 1 < data.len() && data[j + 1].0 <= t {
            j += 1;
        }

        if j + 1 >= data.len() {
            result.push(data.last().unwrap().1);
        } else if t <= data[j].0 {
            result.push(data[j].1);
        } else {
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

/// Generate a Hanning (raised cosine) window of length `n`.
pub fn hanning_window(n: usize) -> Vec<f64> {
    (0..n)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f64 / n as f64).cos()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resample_uniform_basic() {
        let data = vec![(0.0, 1.0), (1.0, 3.0), (2.0, 5.0)];
        let result = resample_uniform(&data, 2.0, 0.0, 2.0);
        // At rate 2.0 Hz from 0 to 2: t = 0.0, 0.5, 1.0, 1.5, 2.0
        assert_eq!(result.len(), 5);
        assert!((result[0] - 1.0).abs() < 1e-9);
        assert!((result[1] - 2.0).abs() < 1e-9);
        assert!((result[2] - 3.0).abs() < 1e-9);
        assert!((result[3] - 4.0).abs() < 1e-9);
        assert!((result[4] - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_hanning_window() {
        let w = hanning_window(4);
        assert_eq!(w.len(), 4);
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
    fn test_median_sample_rate_empty() {
        assert_eq!(median_sample_rate(&[]), 0.0);
        assert_eq!(median_sample_rate(&[(0.0, 1.0)]), 0.0);
    }
}
