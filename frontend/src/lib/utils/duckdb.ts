import type { AsyncDuckDB, AsyncDuckDBConnection } from '@duckdb/duckdb-wasm';

let dbInstance: AsyncDuckDB | null = null;

export async function initDuckDB(): Promise<AsyncDuckDB> {
  if (dbInstance) return dbInstance;
  const duckdb = await import('@duckdb/duckdb-wasm');
  const BUNDLES = duckdb.getJsDelivrBundles();
  const bundle = await duckdb.selectBundle(BUNDLES);
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
    const cols = ['timestamp', ...columns].join(', ');

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

  close(): void {
    if (this.conn) {
      this.conn.close();
      this.conn = null;
    }
  }
}
