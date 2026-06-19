#!/usr/bin/env node
// Copy DuckDB-WASM bundle files from node_modules into static/duckdb/.
// Loading them from the jsDelivr CDN breaks Safari (blob-worker origin issues),
// so we serve them same-origin from the SvelteKit static directory.

import { mkdirSync, copyFileSync, existsSync, statSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, '..');
const src = join(root, 'node_modules', '@duckdb', 'duckdb-wasm', 'dist');
const dest = join(root, 'static', 'duckdb');

const files = [
  'duckdb-mvp.wasm',
  'duckdb-eh.wasm',
  'duckdb-coi.wasm',
  'duckdb-browser-mvp.worker.js',
  'duckdb-browser-eh.worker.js',
  'duckdb-browser-coi.worker.js',
  'duckdb-browser-coi.pthread.worker.js',
];

mkdirSync(dest, { recursive: true });

let copied = 0;
let skipped = 0;
for (const f of files) {
  const s = join(src, f);
  const d = join(dest, f);
  if (!existsSync(s)) {
    console.error(`missing source: ${s}`);
    process.exit(1);
  }
  if (existsSync(d) && statSync(d).mtimeMs >= statSync(s).mtimeMs) {
    skipped++;
    continue;
  }
  copyFileSync(s, d);
  copied++;
}
console.log(`duckdb assets: ${copied} copied, ${skipped} up-to-date`);
