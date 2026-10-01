import { describe, it, expect } from 'vitest';
import { buildParquetUrl, microsToSeconds, resolveTopicRefs } from '../duckdb';

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

describe('resolveTopicRefs', () => {
  const base = '/api/logs/log-a/data';
  const url = (f: string) => `${window.location.origin}${base}/${f}`;

  it('resolves a bare topic name', () => {
    expect(resolveTopicRefs("SELECT * FROM read_parquet('sensor_mag')", base)).toBe(
      `SELECT * FROM read_parquet('${url('sensor_mag.parquet')}')`
    );
  });

  it('resolves a .parquet-suffixed name to the same URL', () => {
    expect(resolveTopicRefs("FROM read_parquet('sensor_mag.parquet')", base)).toBe(
      `FROM read_parquet('${url('sensor_mag.parquet')}')`
    );
  });

  it('resolves multi-instance names', () => {
    expect(resolveTopicRefs("FROM read_parquet('sensor_accel_1')", base)).toBe(
      `FROM read_parquet('${url('sensor_accel_1.parquet')}')`
    );
  });

  it('resolves every reference in a join', () => {
    const sql =
      "FROM read_parquet('vehicle_angular_velocity') v " +
      "ASOF JOIN read_parquet('vehicle_rates_setpoint') s ON v.timestamp >= s.timestamp";
    expect(resolveTopicRefs(sql, base)).toBe(
      `FROM read_parquet('${url('vehicle_angular_velocity.parquet')}') v ` +
        `ASOF JOIN read_parquet('${url('vehicle_rates_setpoint.parquet')}') s ON v.timestamp >= s.timestamp`
    );
  });

  it('tolerates whitespace inside the call', () => {
    expect(resolveTopicRefs("read_parquet( 'cpuload' )", base)).toBe(
      `read_parquet('${url('cpuload.parquet')}')`
    );
  });

  it('leaves absolute paths and URLs untouched', () => {
    for (const ref of [
      "read_parquet('/api/logs/other/data/sensor_mag.parquet')",
      "read_parquet('https://example.com/x.parquet')",
    ]) {
      expect(resolveTopicRefs(ref, base)).toBe(ref);
    }
  });

  it('leaves non-read_parquet SQL untouched', () => {
    const sql = "SELECT 'sensor_mag' AS label, timestamp FROM read_json('x')";
    expect(resolveTopicRefs(sql, base)).toBe(sql);
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
