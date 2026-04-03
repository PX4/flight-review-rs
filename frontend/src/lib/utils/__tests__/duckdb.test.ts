import { describe, it, expect, vi, beforeEach } from 'vitest';
import { buildParquetUrl, microsToSeconds } from '../duckdb';

describe('buildParquetUrl', () => {
  it('builds URL for single-instance topic (multiId=0)', () => {
    expect(buildParquetUrl('/api/logs/abc123/data', 'vehicle_attitude', 0)).toBe(
      '/api/logs/abc123/data/vehicle_attitude.parquet'
    );
  });

  it('builds URL for multi-instance topic (multiId>0)', () => {
    expect(buildParquetUrl('/api/logs/abc123/data', 'sensor_accel', 1)).toBe(
      '/api/logs/abc123/data/sensor_accel_1.parquet'
    );
  });

  it('defaults multiId to 0', () => {
    expect(buildParquetUrl('/api/logs/x/data', 'battery_status')).toBe(
      '/api/logs/x/data/battery_status.parquet'
    );
  });

  it('handles topic names with underscores', () => {
    expect(buildParquetUrl('/api/logs/id/data', 'estimator_sensor_bias', 2)).toBe(
      '/api/logs/id/data/estimator_sensor_bias_2.parquet'
    );
  });
});

describe('microsToSeconds', () => {
  it('converts microsecond values to seconds', () => {
    const fakeCol = {
      length: 3,
      get: (i: number) => [0, 1_000_000, 2_500_000][i],
    };
    const result = microsToSeconds(fakeCol);
    expect(result).toBeInstanceOf(Float64Array);
    expect(result.length).toBe(3);
    expect(result[0]).toBeCloseTo(0);
    expect(result[1]).toBeCloseTo(1.0);
    expect(result[2]).toBeCloseTo(2.5);
  });

  it('handles empty column', () => {
    const fakeCol = { length: 0, get: () => 0 };
    const result = microsToSeconds(fakeCol);
    expect(result.length).toBe(0);
  });

  it('handles BigInt-like values via Number()', () => {
    const fakeCol = {
      length: 2,
      get: (i: number) => [BigInt(5_000_000), BigInt(10_000_000)][i],
    };
    const result = microsToSeconds(fakeCol);
    expect(result[0]).toBeCloseTo(5.0);
    expect(result[1]).toBeCloseTo(10.0);
  });
});

describe('initDuckDB singleton', () => {
  it('module exports initDuckDB and terminateDuckDB', async () => {
    // We can't test actual WASM instantiation in jsdom,
    // but we verify the exports exist and are callable
    const mod = await import('../duckdb');
    expect(typeof mod.initDuckDB).toBe('function');
    expect(typeof mod.terminateDuckDB).toBe('function');
  });
});
