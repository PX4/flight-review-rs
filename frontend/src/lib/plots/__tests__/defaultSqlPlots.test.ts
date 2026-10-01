import { describe, it, expect } from 'vitest';
import type { PlotConfig } from '$lib/types';
import {
  DEFAULT_SQL_PLOTS,
  buildDefaultSqlPlots,
  reconcileDefaultSqlPlots,
} from '../defaultSqlPlots';

// Static lint of the registry: keeps entries portable across logs and
// catches known SQL pitfalls without executing anything.
describe('DEFAULT_SQL_PLOTS registry lint', () => {
  it('has unique, stable, prefixed ids', () => {
    const ids = DEFAULT_SQL_PLOTS.map((d) => d.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const id of ids) {
      expect(id).toMatch(/^sql_[a-z0-9_]+$/);
      // No collision with scratch-plot ids (`sql_<Date.now()>_<n>`).
      expect(id).not.toMatch(/^sql_\d/);
    }
  });

  it('declares non-empty requires', () => {
    for (const d of DEFAULT_SQL_PLOTS) {
      expect(d.requires.length, d.id).toBeGreaterThan(0);
      expect(new Set(d.requires).size, d.id).toBe(d.requires.length);
    }
  });

  it('is a single SELECT/WITH statement', () => {
    for (const d of DEFAULT_SQL_PLOTS) {
      const sql = d.sql.trim();
      expect(sql, d.id).toMatch(/^(select|with)\b/i);
      // No statement separator (a single trailing one would also be cruft).
      expect(sql, d.id).not.toContain(';');
    }
  });

  it('never hardcodes a log-specific data URL', () => {
    for (const d of DEFAULT_SQL_PLOTS) {
      expect(d.sql, d.id).not.toMatch(/\/api\/logs\//);
      expect(d.sql, d.id).not.toMatch(/https?:\/\//);
    }
  });

  it('references topics only by name, all covered by requires', () => {
    for (const d of DEFAULT_SQL_PLOTS) {
      const refs = [...d.sql.matchAll(/read_parquet\('([^']+)'\)/g)].map((m) => m[1]);
      expect(refs.length, `${d.id}: SQL must read at least one topic`).toBeGreaterThan(0);
      for (const ref of refs) {
        // Accept 'name', 'name.parquet', and instance suffixes ('name_1').
        const topic = ref.replace(/\.parquet$/, '').replace(/_\d+$/, '');
        expect(d.requires, `${d.id}: '${ref}' not covered by requires`).toContain(topic);
      }
      for (const topic of d.requires) {
        expect(
          refs.some((r) => r.replace(/\.parquet$/, '').replace(/_\d+$/, '') === topic),
          `${d.id}: requires '${topic}' but the SQL never reads it`
        ).toBe(true);
      }
    }
  });

  it('never equality-joins on timestamp (use ASOF JOIN across topics)', () => {
    for (const d of DEFAULT_SQL_PLOTS) {
      const hasPlainJoin = /(?<!asof\s)\bjoin\b/i.test(d.sql);
      const joinsOnTimestampEquality = /\bon\b[^()]*"?timestamp"?\s*=/i.test(d.sql);
      expect(
        hasPlainJoin && joinsOnTimestampEquality,
        `${d.id}: plain JOIN ... ON timestamp = — topics sample at different rates; use ASOF JOIN`
      ).toBe(false);
    }
  });

  it('window frames order on a signed cast of timestamp', () => {
    for (const d of DEFAULT_SQL_PLOTS) {
      if (!/\bover\b|\bwindow\b/i.test(d.sql)) continue;
      if (!/\brange\s+between\b/i.test(d.sql)) continue;
      expect(
        /cast\s*\(\s*"?timestamp"?\s+as\s+bigint\s*\)/i.test(d.sql),
        `${d.id}: RANGE frame over raw UINT64 timestamp underflows near t=0 — ORDER BY CAST(timestamp AS BIGINT)`
      ).toBe(true);
    }
  });
});

function scratchSqlPlot(): PlotConfig {
  return {
    id: 'sql_1753430000000_1',
    topic: '',
    multiId: 0,
    fields: [],
    yLabel: 'My scratch plot',
    colors: [],
    kind: 'sql',
    sql: "SELECT timestamp, x FROM read_parquet('sensor_mag') ORDER BY timestamp",
  };
}

function topicPlot(): PlotConfig {
  return {
    id: 'accel',
    topic: 'sensor_combined',
    multiId: 0,
    fields: ['accelerometer_m_s2[0]'],
    yLabel: 'Acceleration (m/s²)',
    colors: ['#818cf8'],
  };
}

const REGISTRY_FIRST = DEFAULT_SQL_PLOTS[0];
const TOPICS_ALL = new Set(DEFAULT_SQL_PLOTS.flatMap((d) => d.requires));
const TOPICS_NONE = new Set<string>();

describe('buildDefaultSqlPlots', () => {
  it('emits a PlotConfig per available registry entry', () => {
    const plots = buildDefaultSqlPlots(TOPICS_ALL);
    expect(plots.length).toBe(DEFAULT_SQL_PLOTS.length);
    const p = plots.find((x) => x.id === REGISTRY_FIRST.id)!;
    expect(p.kind).toBe('sql');
    expect(p.sql).toBe(REGISTRY_FIRST.sql);
    expect(p.yLabel).toBe(REGISTRY_FIRST.yLabel);
    expect(p.topic).toBe('');
    expect(p.fields).toEqual([]);
  });

  it('gates on required topics', () => {
    expect(buildDefaultSqlPlots(TOPICS_NONE)).toEqual([]);
  });
});

describe('reconcileDefaultSqlPlots', () => {
  it('overwrites a saved registry plot from the repo definition (repo wins)', () => {
    const stale: PlotConfig = {
      ...buildDefaultSqlPlots(TOPICS_ALL).find((p) => p.id === REGISTRY_FIRST.id)!,
      sql: 'SELECT 1 AS timestamp, 2 AS stale',
      yLabel: 'Stale label',
      colors: ['#000000'],
      minimized: true,
    };
    const out = reconcileDefaultSqlPlots([stale], TOPICS_ALL);
    const p = out.find((x) => x.id === REGISTRY_FIRST.id)!;
    expect(p.sql).toBe(REGISTRY_FIRST.sql);
    expect(p.yLabel).toBe(REGISTRY_FIRST.yLabel);
    expect(p.colors).toEqual(REGISTRY_FIRST.colors ?? []);
    // Layout state is the user's — preserved.
    expect(p.minimized).toBe(true);
  });

  it('drops a registry plot the log can no longer satisfy', () => {
    const saved = buildDefaultSqlPlots(TOPICS_ALL);
    const out = reconcileDefaultSqlPlots(saved, TOPICS_NONE);
    expect(out.some((p) => p.id === REGISTRY_FIRST.id)).toBe(false);
  });

  it('appends registry entries missing from the saved layout', () => {
    const out = reconcileDefaultSqlPlots([topicPlot()], TOPICS_ALL);
    expect(out[0].id).toBe('accel');
    expect(out.some((p) => p.id === REGISTRY_FIRST.id)).toBe(true);
  });

  it('passes through personal scratch SQL plots and topic plots untouched', () => {
    const scratch = scratchSqlPlot();
    const topic = topicPlot();
    const out = reconcileDefaultSqlPlots([scratch, topic], TOPICS_NONE);
    expect(out).toEqual([scratch, topic]);
  });

  it('preserves saved order for present entries', () => {
    const registry = buildDefaultSqlPlots(TOPICS_ALL).find((p) => p.id === REGISTRY_FIRST.id)!;
    const out = reconcileDefaultSqlPlots([registry, topicPlot()], TOPICS_ALL);
    expect(out.map((p) => p.id)).toEqual([REGISTRY_FIRST.id, 'accel', ...DEFAULT_SQL_PLOTS.slice(1).map((d) => d.id)]);
  });
});
