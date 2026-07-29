import type { AsyncDuckDB, AsyncDuckDBConnection, DuckDBBundles } from '@duckdb/duckdb-wasm';

/**
 * Plottable (numeric) result columns, by declared type rather than values —
 * robust for computed/aliased and all-null columns. Matches stable Arrow
 * type ids (no apache-arrow import): Int=2, Float=3, Bool=6, Decimal=7.
 */
const NUMERIC_ARROW_TYPE_IDS = new Set<number>([2, 3, 6, 7]);

function isNumericField(field: { typeId: number }): boolean {
  return NUMERIC_ARROW_TYPE_IDS.has(field.typeId);
}

let dbInstance: AsyncDuckDB | null = null;

// Same-origin bundle map. Files are served from /duckdb/* — by the Vite dev
// plugin in dev, and from static/duckdb/ in prod (synced at build time).
// Loading from a CDN breaks Safari because the worker ends up with an opaque
// origin and can't fetch the .wasm module.
function getBundles(): DuckDBBundles {
  return {
    mvp: {
      mainModule: '/duckdb/duckdb-mvp.wasm',
      mainWorker: '/duckdb/duckdb-browser-mvp.worker.js',
    },
    eh: {
      mainModule: '/duckdb/duckdb-eh.wasm',
      mainWorker: '/duckdb/duckdb-browser-eh.worker.js',
    },
    coi: {
      mainModule: '/duckdb/duckdb-coi.wasm',
      mainWorker: '/duckdb/duckdb-browser-coi.worker.js',
      pthreadWorker: '/duckdb/duckdb-browser-coi.pthread.worker.js',
    },
  };
}

export async function initDuckDB(): Promise<AsyncDuckDB> {
  if (dbInstance) return dbInstance;
  const duckdb = await import('@duckdb/duckdb-wasm');
  const bundle = await duckdb.selectBundle(getBundles());

  const worker = new Worker(bundle.mainWorker!);
  const logger = new duckdb.ConsoleLogger();
  const db = new duckdb.AsyncDuckDB(logger, worker);
  await db.instantiate(bundle.mainModule, bundle.pthreadWorker);
  dbInstance = db;
  return db;
}

export function terminateDuckDB(): void {
  if (dbInstance) {
    dbInstance.terminate();
    dbInstance = null;
  }
}

/**
 * Build a Parquet URL for a given topic and multi_id.
 * Exported for testability.
 */
export function buildParquetUrl(baseUrl: string, topic: string, multiId: number = 0): string {
  const filename = multiId > 0 ? `${topic}_${multiId}.parquet` : `${topic}.parquet`;
  return `${baseUrl}/${filename}`;
}

/**
 * Convert microsecond timestamps to seconds (Float64Array).
 * Exported for testability.
 */
export function microsToSeconds(values: { length: number; get(i: number): unknown }): Float64Array {
  const out = new Float64Array(values.length);
  for (let i = 0; i < values.length; i++) {
    out[i] = Number(values.get(i)) / 1e6;
  }
  return out;
}

/**
 * Extract a numeric column to Float64Array.
 */
function columnToFloat64(col: { length: number; get(i: number): unknown }): Float64Array {
  const out = new Float64Array(col.length);
  for (let i = 0; i < col.length; i++) {
    out[i] = Number(col.get(i));
  }
  return out;
}

/**
 * Resolve bare topic refs — `read_parquet('sensor_mag')` or
 * `('sensor_mag.parquet')` — to the log's absolute Parquet URL, keeping raw
 * SQL log-agnostic. Full URLs/paths pass through. Exported for testability.
 */
export function resolveTopicRefs(sql: string, baseUrl: string): string {
  return sql.replace(
    /read_parquet\(\s*'([A-Za-z_][A-Za-z0-9_]*)(?:\.parquet)?'\s*\)/g,
    (_m, stem) => `read_parquet('${window.location.origin}${baseUrl}/${stem}.parquet')`
  );
}

export class LogSession {
  private db: AsyncDuckDB;
  private conn: AsyncDuckDBConnection | null = null;
  private baseUrl: string;

  constructor(db: AsyncDuckDB, logId: string) {
    this.db = db;
    this.baseUrl = `/api/logs/${logId}/data`;
  }

  async getConnection(): Promise<AsyncDuckDBConnection> {
    if (!this.conn) {
      this.conn = await this.db.connect();
    }
    return this.conn;
  }

  private parquetUrl(topic: string, multiId: number = 0): string {
    return buildParquetUrl(this.baseUrl, topic, multiId);
  }

  async queryTopic(
    topic: string,
    columns: string[],
    options?: { timeRange?: [number, number]; multiId?: number; maxPoints?: number }
  ): Promise<{ timestamps: Float64Array; series: Float64Array[] } | null> {
    const conn = await this.getConnection();
    const url = `${window.location.origin}${this.parquetUrl(topic, options?.multiId ?? 0)}`;
    const cols = ['timestamp', ...columns.map((c) => `"${c}"`)].join(', ');

    let sql: string;
    if (options?.timeRange) {
      const [start, end] = options.timeRange;
      sql = `SELECT ${cols} FROM read_parquet('${url}') WHERE timestamp >= ${start} AND timestamp <= ${end} ORDER BY timestamp`;
    } else {
      sql = `SELECT ${cols} FROM read_parquet('${url}') ORDER BY timestamp`;
    }

    try {
      const result = await conn.query(sql);
      const tsCol = result.getChild('timestamp');
      if (!tsCol || tsCol.length === 0) return null;

      const timestamps = microsToSeconds(tsCol);

      const series: Float64Array[] = [];
      for (const col of columns) {
        const child = result.getChild(col);
        if (!child) {
          series.push(new Float64Array(tsCol.length));
          continue;
        }
        series.push(columnToFloat64(child));
      }

      return { timestamps, series };
    } catch (e) {
      console.error(`DuckDB query failed for ${topic}:`, e);
      return null;
    }
  }

  async getTopicSchema(topic: string, multiId: number = 0): Promise<{ name: string; type: string }[]> {
    const conn = await this.getConnection();
    const url = `${window.location.origin}${this.parquetUrl(topic, multiId)}`;
    try {
      const result = await conn.query(`DESCRIBE SELECT * FROM read_parquet('${url}')`);
      const names = result.getChild('column_name');
      const types = result.getChild('column_type');
      if (!names || !types) return [];
      const schema: { name: string; type: string }[] = [];
      for (let i = 0; i < names.length; i++) {
        const name = names.get(i) as string;
        if (name !== 'timestamp') {
          schema.push({ name, type: types.get(i) as string });
        }
      }
      return schema;
    } catch (e) {
      console.error(`Schema query failed for ${topic}:`, e);
      return [];
    }
  }

  /**
   * Run arbitrary SQL (body unrestricted; topic refs resolved by
   * `resolveTopicRefs`, `timestamp` stays UINT64 — cast it for window
   * frames). Only the result columns follow a chart contract: x = the
   * `timestamp` column (µs→s) or else the first column, which must be
   * numeric; every other numeric column is a series labelled by name,
   * non-numeric columns are skipped. Returns null on zero rows; throws
   * DuckDB/shape errors for the caller to surface.
   */
  async querySql(
    sql: string
  ): Promise<{ x: Float64Array; series: Float64Array[]; labels: string[]; xIsTime: boolean } | null> {
    const conn = await this.getConnection();
    const result = await conn.query(resolveTopicRefs(sql, this.baseUrl));
    if (result.numRows === 0) return null;

    const fields = result.schema.fields as Array<{ name: string; typeId: number; type: unknown }>;
    if (fields.length < 2) {
      throw new Error('Query must return at least 2 columns: an x-axis column plus one or more series.');
    }

    const tsIdx = fields.findIndex((f) => f.name === 'timestamp');
    const xIdx = tsIdx >= 0 ? tsIdx : 0;
    const xField = fields[xIdx];
    if (!isNumericField(xField)) {
      throw new Error(
        `x-axis column "${xField.name}" is not numeric (${String(xField.type)}). ` +
          `Put a numeric column first, or name your time column "timestamp".`
      );
    }
    // Index-based access throughout: name lookups return the first match, so
    // duplicate column names (common in joins) would alias the wrong data.
    const xCol = result.getChildAt(xIdx);
    if (!xCol) {
      throw new Error(`Failed to read x-axis column "${xField.name}" from the result.`);
    }
    // Only a column literally named `timestamp` joins the shared time axis /
    // cross-plot sync; any other x would corrupt the shared seconds range.
    const xIsTime = xField.name === 'timestamp';
    const x = xIsTime ? microsToSeconds(xCol) : columnToFloat64(xCol);

    const series: Float64Array[] = [];
    const labels: string[] = [];
    fields.forEach((field, i) => {
      if (i === xIdx) return;
      if (!isNumericField(field)) return;
      const col = result.getChildAt(i);
      if (!col) return;
      series.push(columnToFloat64(col));
      labels.push(field.name);
    });
    if (series.length === 0) {
      throw new Error('Query returned no numeric columns to plot beyond the x-axis.');
    }
    return { x, series, labels, xIsTime };
  }

  close(): void {
    if (this.conn) {
      this.conn.close();
      this.conn = null;
    }
  }
}
