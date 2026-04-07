/**
 * Viridis colormap (256 entries) — perceptually uniform sequential colormap
 * matching Bokeh's Viridis256 / matplotlib's viridis.
 *
 * The full 256-entry table would add ~3KB. Instead we sample 17 stops at
 * even intervals from the official lookup and linearly interpolate. The
 * resulting colors are visually indistinguishable from the full table for
 * a heatmap rendering.
 */

// 17 stops at i/16 from the matplotlib viridis lookup table.
const STOPS: Array<[number, number, number]> = [
  [68, 1, 84],     // 0.000
  [72, 26, 108],   // 0.0625
  [71, 47, 124],   // 0.125
  [65, 68, 135],   // 0.1875
  [57, 86, 140],   // 0.25
  [49, 104, 142],  // 0.3125
  [42, 120, 142],  // 0.375
  [37, 134, 142],  // 0.4375
  [32, 145, 140],  // 0.5
  [31, 158, 137],  // 0.5625
  [37, 171, 130],  // 0.625
  [55, 184, 120],  // 0.6875
  [85, 196, 104],  // 0.75
  [121, 206, 84],  // 0.8125
  [161, 215, 60],  // 0.875
  [201, 222, 35],  // 0.9375
  [253, 231, 37],  // 1.000
];

/**
 * Sample the viridis colormap at t ∈ [0, 1]. Returns [r, g, b] in 0..255.
 */
export function viridis(t: number): [number, number, number] {
  if (!(t > 0)) return STOPS[0];
  if (t >= 1) return STOPS[STOPS.length - 1];
  const scaled = t * (STOPS.length - 1);
  const i = Math.floor(scaled);
  const frac = scaled - i;
  const a = STOPS[i];
  const b = STOPS[i + 1];
  return [
    Math.round(a[0] + (b[0] - a[0]) * frac),
    Math.round(a[1] + (b[1] - a[1]) * frac),
    Math.round(a[2] + (b[2] - a[2]) * frac),
  ];
}
