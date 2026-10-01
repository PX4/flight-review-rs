import type { PlotConfig } from '$lib/types';

/**
 * A repo-committed SQL default plot. Definitions live in git, never in
 * localStorage; the Edit Plots SQL editor is the scratchpad before
 * committing an entry here. Rules (enforced by defaultSqlPlots.test.ts):
 * stable `id`; topic names only, no `/api/logs/<id>/...` URLs; `requires`
 * lists every topic read; `ASOF JOIN` across topics; window frames order on
 * `CAST(timestamp AS BIGINT)`.
 */
export interface DefaultSqlPlot {
  /** Stable unique id, prefixed `sql_`. Never reuse or rename. */
  id: string;
  yLabel: string;
  /** Result contract: `timestamp` col (µs) = x; other numeric cols = series. */
  sql: string;
  /** Topics the SQL reads — availability gate per log. */
  requires: string[];
  /** Optional; PlotStrip falls back to its palette. */
  colors?: string[];
}

export const DEFAULT_SQL_PLOTS: DefaultSqlPlot[] = [
  // Roll rate vs setpoint on one plot (Flight Review v1 style) — needs a
  // cross-topic ASOF JOIN, which field selection can't express.
  {
    id: 'sql_roll_rate_tracking',
    yLabel: 'Roll Rate Tracking (rad/s)',
    sql:
      `SELECT v.timestamp,\n` +
      `       v."xyz[0]" AS roll_rate,\n` +
      `       s.roll     AS roll_rate_setpoint\n` +
      `FROM read_parquet('vehicle_angular_velocity') v\n` +
      `ASOF JOIN read_parquet('vehicle_rates_setpoint') s\n` +
      `  ON v.timestamp >= s.timestamp\n` +
      `ORDER BY v.timestamp`,
    requires: ['vehicle_angular_velocity', 'vehicle_rates_setpoint'],
    colors: ['#818cf8', '#fbbf24'],
  },
];

function makeSqlPlot(def: DefaultSqlPlot): PlotConfig {
  return {
    id: def.id,
    topic: '',
    multiId: 0,
    fields: [],
    yLabel: def.yLabel,
    colors: def.colors ?? [],
    kind: 'sql',
    sql: def.sql,
  };
}

function isAvailable(def: DefaultSqlPlot, availableTopics: Set<string>): boolean {
  return def.requires.every((t) => availableTopics.has(t));
}

/** Registry plots for a fresh layout, gated on the log's available topics. */
export function buildDefaultSqlPlots(availableTopics: Set<string>): PlotConfig[] {
  return DEFAULT_SQL_PLOTS.filter((d) => isAvailable(d, availableTopics)).map(makeSqlPlot);
}

/**
 * Reconcile a saved layout with the registry ("repo wins"): matched ids get
 * their definition from the repo (localStorage keeps only presence/order/
 * minimized); unsatisfiable entries drop; missing ones append; everything
 * else passes through.
 */
export function reconcileDefaultSqlPlots(
  plots: PlotConfig[],
  availableTopics: Set<string>
): PlotConfig[] {
  const byId = new Map(DEFAULT_SQL_PLOTS.map((d) => [d.id, d]));
  const out: PlotConfig[] = [];
  for (const p of plots) {
    const def = p.kind === 'sql' ? byId.get(p.id) : undefined;
    if (!def) {
      out.push(p);
      continue;
    }
    if (!isAvailable(def, availableTopics)) continue;
    out.push({ ...p, sql: def.sql, yLabel: def.yLabel, colors: def.colors ?? [] });
  }
  const present = new Set(out.map((p) => p.id));
  for (const def of DEFAULT_SQL_PLOTS) {
    if (!present.has(def.id) && isAvailable(def, availableTopics)) {
      out.push(makeSqlPlot(def));
    }
  }
  return out;
}
