import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vitest/config';
import { readFileSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { join } from 'node:path';
import type { Plugin } from 'vite';

// Build-time version info baked into the bundle and shown in the nav's version
// panel. Git calls are guarded so `npm run dev` still works outside a repo or
// without git installed.
const pkgVersion = (() => {
  try {
    return JSON.parse(readFileSync(join(__dirname, 'package.json'), 'utf-8')).version ?? 'unknown';
  } catch {
    return 'unknown';
  }
})();

function safeGit(args: string): string {
  try {
    return execSync(`git ${args}`, { cwd: __dirname, stdio: ['ignore', 'pipe', 'ignore'] })
      .toString()
      .trim();
  } catch {
    return 'unknown';
  }
}

const gitSha = (() => {
  const sha = safeGit('rev-parse --short HEAD');
  if (sha === 'unknown') return sha;
  const dirty = safeGit('status --porcelain');
  return dirty && dirty !== 'unknown' ? `${sha}-dirty` : sha;
})();

const buildTime = new Date().toISOString();

/**
 * Serve DuckDB-WASM bundle files (workers, wasm, source maps) from node_modules
 * under /duckdb/* during dev.
 *
 * Loading workers/wasm from the jsDelivr CDN breaks in Safari: the worker is
 * created from a blob URL with an opaque origin, and subsequent fetches for
 * the .wasm module are blocked. Serving everything same-origin avoids this.
 */
function duckdbAssets(): Plugin {
  const DUCKDB_RE = /^\/duckdb\/(duckdb-[A-Za-z0-9._-]+)$/;
  const distDir = join(
    __dirname,
    'node_modules',
    '@duckdb',
    'duckdb-wasm',
    'dist',
  );

  const contentType = (file: string): string => {
    if (file.endsWith('.wasm')) return 'application/wasm';
    if (file.endsWith('.js')) return 'application/javascript';
    if (file.endsWith('.map')) return 'application/json';
    return 'application/octet-stream';
  };

  return {
    name: 'duckdb-assets',
    apply: 'serve', // dev only
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const match = req.url && DUCKDB_RE.exec(req.url.split('?')[0]);
        if (!match) return next();
        try {
          const content = readFileSync(join(distDir, match[1]));
          res.setHeader('Content-Type', contentType(match[1]));
          res.setHeader('Cache-Control', 'no-cache');
          res.end(content);
        } catch {
          next();
        }
      });
    },
  };
}

export default defineConfig({
  plugins: [duckdbAssets(), tailwindcss(), sveltekit()],
  define: {
    __APP_VERSION__: JSON.stringify(pkgVersion),
    __GIT_SHA__: JSON.stringify(gitSha),
    __BUILD_TIME__: JSON.stringify(buildTime),
  },
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
