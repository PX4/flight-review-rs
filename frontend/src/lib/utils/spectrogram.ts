/**
 * Short-Time Fourier Transform (STFT) for spectrogram computation.
 *
 * Mirrors scipy.signal.spectrogram(window='hann', nperseg=N, noverlap=N/2,
 * scaling='density') so the output matches v1 flight_review's PSD plot.
 *
 * Output shape: { freqs[nFreq], times[nTime], psd[nFreq * nTime] (row-major
 * by frequency, i.e. psd[f * nTime + t]) }
 */

import FFT from 'fft.js';

export interface SpectrogramResult {
  /** Frequencies in Hz, length = nperseg / 2 + 1 */
  freqs: Float64Array;
  /** Center times of each window in seconds (relative to first sample) */
  times: Float64Array;
  /** PSD values, row-major: psd[f * nTime + t] */
  psd: Float64Array;
  nFreq: number;
  nTime: number;
}

/** Build a periodic Hann window of the given length (matches scipy default). */
function hannWindow(n: number): Float64Array {
  const w = new Float64Array(n);
  for (let i = 0; i < n; i++) {
    w[i] = 0.5 * (1 - Math.cos((2 * Math.PI * i) / n));
  }
  return w;
}

/**
 * Compute the PSD spectrogram of a single real-valued signal using
 * scipy-compatible 'density' scaling.
 */
export function computePsdSpectrogram(
  signal: Float64Array | number[],
  fs: number,
  nperseg = 256,
  noverlap = 128
): SpectrogramResult | null {
  if (signal.length < nperseg) return null;
  if (nperseg <= 0 || (nperseg & (nperseg - 1)) !== 0) {
    // fft.js requires a power of two
    return null;
  }

  const win = hannWindow(nperseg);
  let winSumSq = 0;
  for (let i = 0; i < nperseg; i++) winSumSq += win[i] * win[i];

  // density scale: 1 / (Fs * sum(win^2))
  const scale = 1 / (fs * winSumSq);

  const step = nperseg - noverlap;
  const nTime = Math.floor((signal.length - nperseg) / step) + 1;
  const nFreq = nperseg / 2 + 1;

  const fft = new FFT(nperseg);
  const segIn = new Float64Array(nperseg);
  const out = fft.createComplexArray() as number[]; // length = 2*nperseg

  const psd = new Float64Array(nFreq * nTime);

  for (let t = 0; t < nTime; t++) {
    const start = t * step;
    // window the segment
    for (let i = 0; i < nperseg; i++) {
      segIn[i] = signal[start + i] * win[i];
    }
    fft.realTransform(out, segIn as any);
    // Note: realTransform only fills the first half (n complex pairs); use
    // those directly. out layout = [re0, im0, re1, im1, ..., re(n/2), im(n/2)].
    for (let k = 0; k < nFreq; k++) {
      const re = out[2 * k];
      const im = out[2 * k + 1];
      let p = (re * re + im * im) * scale;
      // One-sided: double everything except DC (k=0) and Nyquist (k=nFreq-1)
      if (k > 0 && k < nFreq - 1) p *= 2;
      psd[k * nTime + t] = p;
    }
  }

  // Frequency bins
  const freqs = new Float64Array(nFreq);
  for (let k = 0; k < nFreq; k++) freqs[k] = (k * fs) / nperseg;

  // Center times of each window (matches scipy output)
  const times = new Float64Array(nTime);
  for (let t = 0; t < nTime; t++) {
    times[t] = (t * step + nperseg / 2) / fs;
  }

  return { freqs, times, psd, nFreq, nTime };
}

/**
 * Sum multiple PSD spectrograms element-wise (matches v1's per-axis sum).
 * All inputs must have the same shape.
 */
export function sumPsd(specs: SpectrogramResult[]): SpectrogramResult | null {
  if (specs.length === 0) return null;
  const first = specs[0];
  const out = new Float64Array(first.psd.length);
  for (const s of specs) {
    if (s.psd.length !== out.length) return null;
    for (let i = 0; i < out.length; i++) out[i] += s.psd[i];
  }
  return { ...first, psd: out };
}

/** Convert PSD to dB in-place: 20 → 10*log10(x). Replaces -inf with the
 *  smallest finite value found, matching v1 behavior. */
export function psdToDb(psd: Float64Array): { min: number; max: number } {
  let min = Infinity;
  let max = -Infinity;
  for (let i = 0; i < psd.length; i++) {
    const v = psd[i];
    if (v > 0) {
      const db = 10 * Math.log10(v);
      psd[i] = db;
      if (db < min) min = db;
      if (db > max) max = db;
    } else {
      psd[i] = -Infinity;
    }
  }
  if (!Number.isFinite(min)) {
    min = -120;
    max = 0;
  }
  // Replace -inf with finite min
  for (let i = 0; i < psd.length; i++) {
    if (!Number.isFinite(psd[i])) psd[i] = min;
  }
  return { min, max };
}
