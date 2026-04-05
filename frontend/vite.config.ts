import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vitest/config';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import type { Plugin } from 'vite';

/**
 * Serve DuckDB worker source-map files from node_modules during dev.
 *
 * The DuckDB WASM worker is loaded via a blob URL (fetched from a CDN).
 * The worker script contains a `//# sourceMappingURL=<file>.map` comment,
 * which the browser resolves relative to the page origin, hitting the Vite
 * dev server and producing a 404. This plugin intercepts those requests and
 * serves the corresponding .map file from the installed package.
 */
function duckdbWorkerSourcemaps(): Plugin {
  const DUCKDB_MAP_RE = /^\/(duckdb-browser-.+\.worker\.js\.map)$/;
  const distDir = join(
    __dirname,
    'node_modules',
    '@duckdb',
    'duckdb-wasm',
    'dist',
  );

  return {
    name: 'duckdb-worker-sourcemaps',
    apply: 'serve', // dev only
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const match = req.url && DUCKDB_MAP_RE.exec(req.url);
        if (!match) return next();
        try {
          const content = readFileSync(join(distDir, match[1]));
          res.setHeader('Content-Type', 'application/json');
          res.end(content);
        } catch {
          next();
        }
      });
    },
  };
}

export default defineConfig({
  plugins: [duckdbWorkerSourcemaps(), tailwindcss(), sveltekit()],
  server: {
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
  build: {
    target: 'es2020',
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts'],
    server: {
      deps: {
        inline: [/svelte/],
      },
    },
  },
  resolve: {
    conditions: ['browser'],
  },
});
