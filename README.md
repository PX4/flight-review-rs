# Flight Review v2

## Table of Contents

- [Introduction](#introduction)
- [Architecture](#architecture)
  - [Upload Workflow](#upload-workflow)
  - [Backend Dependencies](#backend-dependencies)
  - [Converter Crate (`flight-review`)](#converter-crate-flight-review)
  - [Server Crate (`flight-review-server`)](#server-crate-flight-review-server)
  - [Frontend](#frontend)
  - [CLI Tool (`ulog-convert`)](#cli-tool-ulog-convert)
  - [Two Paths](#two-paths)
  - [Workspace Layout](#workspace-layout)
  - [What Gets Stored Per Log](#what-gets-stored-per-log)
  - [API](#api)
- [Tech Stack](#tech-stack)
- [Build](#build)
  - [Development](#development)
  - [Seeding Data](#seeding-data)
  - [Release Build](#release-build)
  - [Feature Flags](#feature-flags)
  - [Database Support](#database-support)
  - [Storage Support](#storage-support)
- [Deploy](#deploy)
  - [Minimal (single binary)](#minimal-single-binary)
  - [Docker](#docker)
  - [Production (AWS)](#production-aws)
- [Migrate from v1](#migrate-from-v1)
- [CLI](#cli)
- [Upload Context Fields](#upload-context-fields)
- [Diagnostics](#diagnostics)
  - [Available Analyzers](#available-analyzers)
  - [Adding a New Analyzer](#adding-a-new-analyzer)
- [Roadmap](#roadmap)
- [License](#license)

## Introduction

Flight Review v2 is a complete rewrite of [PX4 Flight Review](https://github.com/PX4/flight_review) in Rust. It replaces the "parse every time you view" model with a **parse-once-store-review** architecture: ULog files are converted to per-topic [Parquet](https://parquet.apache.org/) files and a rich metadata JSON at upload time, then served as static files for client-side analysis via [DuckDB](https://duckdb.org/)-WASM. The frontend is a SvelteKit single-page application that queries Parquet files directly via DuckDB-WASM in the browser. The result is sub-second log viewing with zero server-side compute, support for SQLite or Postgres, and local filesystem or S3 storage -- deployable as a single binary or a Docker container.

## Architecture

### Upload Workflow

```
Upload .ulg --> Rust converter --> Per-topic Parquet + metadata.json --> Storage
```

At upload time the server parses the ULog file once, writes compressed Parquet files (one per topic) and a `metadata.json` containing all extracted metadata and flight analysis results. From that point on the browser queries Parquet directly via DuckDB-WASM and HTTP Range requests -- the server never re-parses the log.

### Backend Dependencies

The converter and server are built on these key libraries:

- [px4-ulog-rs](https://github.com/Auterion/px4-ulog-rs) (Auterion) -- streaming ULog parser
- [Apache Arrow](https://arrow.apache.org/) / [Parquet](https://parquet.apache.org/) -- columnar format and serialization
- [rustfft](https://github.com/LabBros/rustfft) -- FFT for PID analysis

On top of these, the workspace provides two crates: `flight-review` (converter library + CLI) and `flight-review-server` (HTTP API).

### Converter Crate (`flight-review`)

The converter library handles all ULog processing:

- ULog parsing via px4-ulog-rs
- Per-topic Parquet conversion with ZSTD compression
- Metadata extraction (all 13 ULog message types)
- Flight analysis (modes, stats, battery, GPS quality, vibration, param diff, GPS track)
- Diagnostic analyzers (motor failure, GPS interference, battery brownout, EKF failure, RC loss)
- PID step response analysis (Wiener deconvolution)

### Server Crate (`flight-review-server`)

The HTTP API server built on axum:

- axum-based REST API
- Upload, list, search, get, delete endpoints
- File serving with HTTP Range requests (for DuckDB-WASM)
- Pluggable database (SQLite, Postgres)
- Pluggable storage (local filesystem, S3)
- v1 migration and lazy conversion

### Frontend

The web frontend is a SvelteKit 5 single-page application using Svelte 5 runes for reactivity. It is built with the static adapter, producing a set of static files that can be served by the backend or any static host.

Key technologies:

- **SvelteKit 5** with static adapter -- client-side routing, no SSR
- **Svelte 5 runes** -- `$state`, `$derived`, `$effect` for reactive state
- **Tailwind CSS v4** -- utility-first styling via Vite plugin
- **uPlot** -- high-performance time-series plotting for sensor data
- **DuckDB-WASM** -- in-browser SQL queries over Parquet files via HTTP Range requests
- **Mapbox GL JS** -- interactive GPS track maps
- **Chart.js** -- statistical charts on the stats page
- **TypeScript** throughout

### CLI Tool (`ulog-convert`)

`ulog-convert` is a standalone command-line tool for converting, diagnosing, and analyzing PX4 ULog files. No server, no database -- purely file-based. Designed for both individual file processing and batch workflows over entire flight log datasets.

Key capabilities:

- **Convert** ULog to per-topic Parquet files with metadata
- **Diagnose** flight anomalies (motor failure, GPS interference, battery brownout, EKF failure, RC loss)
- **Analyze** signal processing (PID step response via Wiener deconvolution)
- **Batch process** directories of ULog files with parallel execution via rayon

Every conversion produces a `manifest.json` that maps the output (source file, topics to Parquet paths, diagnostic results). Batch conversions additionally produce an `index.json` at the output root that indexes all converted logs.

### Two Paths

There are two ways to use the project -- through the server for production deployments, or through the CLI for local and scripted workflows:

```
                    +-----------------------+
                    |   .ulg file input     |
                    +----------+------------+
                               |
                    +----------+------------+
                    |                       |
              +-----v------+       +--------v--------+
              | ulog-convert|      | flight-review-  |
              |   (CLI)    |       |    server        |
              +-----+------+       +--------+--------+
                    |                       |
              Local files            API + Storage
              (Parquet +             (S3 / local fs
               metadata.json)        + SQLite/Postgres)
```

### Workspace Layout

```
flight-review-rs/
├── crates/
│   ├── converter/          # Library + CLI
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── converter.rs    # ULog --> per-topic ZSTD Parquet files
│   │   │   ├── metadata.rs     # All 13 ULog message types --> metadata.json
│   │   │   ├── analysis.rs     # Flight modes, stats, battery, GPS, vibration, param diff
│   │   │   ├── diagnostics/    # Diagnostic analyzers (motor, GPS, battery, EKF, RC)
│   │   │   ├── signal_processing/ # Signal processing framework (PID step response, DSP)
│   │   │   ├── pid_analysis.rs # Backward-compat facade for signal_processing
│   │   │   └── bin/
│   │   │       └── ulog_convert.rs
│   │   ├── benches/            # Criterion benchmarks
│   │   ├── tests/fixtures/     # ULog test fixtures (normal + failure cases)
│   │   └── Cargo.toml
│   └── server/             # HTTP API server
│       ├── src/
│       │   ├── main.rs
│       │   ├── lib.rs
│       │   ├── api/        # Upload, list, get, delete, file serving (Range requests)
│       │   ├── db/         # LogStore trait -- SQLite and Postgres backends
│       │   └── storage/    # object_store -- local filesystem and S3
│       └── Cargo.toml
├── frontend/               # SvelteKit web application
│   ├── src/
│   │   ├── routes/         # SvelteKit pages and layouts
│   │   ├── lib/            # Components, stores, utilities
│   │   └── app.css         # Tailwind entry point
│   ├── package.json
│   ├── svelte.config.js
│   └── vite.config.ts
├── scripts/
│   ├── download-logs.sh    # Seed local instance with real logs from v1
│   └── ci/
│       └── check-analyzer.sh  # CI validation for new diagnostic analyzers
├── Dockerfile
├── Cargo.toml              # Workspace root
└── README.md
```

### What Gets Stored Per Log

All files live under a single UUID directory:

```
<uuid>/
├── metadata.json           # Metadata + flight analysis + diagnostics
├── <uuid>.ulg              # Original upload
├── vehicle_attitude.parquet
├── sensor_combined.parquet
├── battery_status.parquet
└── ...                     # One Parquet file per ULog topic
```

The `metadata.json` includes flight modes, stats, battery summary, GPS quality, vibration status, GPS track, parameter diffs, and diagnostic results. Diagnostics are automatically detected during upload and included in the `analysis.diagnostics` array.

### API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/health` | Health check |
| `POST` | `/api/upload` | Multipart upload -- accepts `.ulg` file + optional context fields |
| `GET` | `/api/logs` | List/search logs (paginated, filtered by hardware, diagnostics, etc.) |
| `GET` | `/api/logs/facets` | Distinct values for filterable fields (hardware, vehicle type, etc.) |
| `GET` | `/api/logs/:id` | Single log record |
| `GET` | `/api/logs/:id/track` | GeoJSON GPS track for a single log |
| `DELETE` | `/api/logs/:id?token=<token>` | Delete log (requires delete token from upload) |
| `GET` | `/api/logs/:id/data/:filename` | Serve Parquet/JSON/ULG files with HTTP Range support |
| `GET` | `/api/stats` | Aggregate statistics (upload counts, vehicle types, etc.) |

## Tech Stack

| Layer | Stack |
|-------|-------|
| Backend | Rust, axum, SQLite/Postgres, object_store |
| Converter | px4-ulog-rs, Apache Arrow/Parquet |
| Frontend | SvelteKit 5, Svelte 5, Tailwind v4, TypeScript |
| Visualization | uPlot, Chart.js, Mapbox GL JS |
| Client-side data | DuckDB-WASM, Apache Arrow |

## Build

We support Linux, macOS, and any platform Rust targets. The project compiles to native binaries with no runtime dependencies beyond libc. Both the CLI tool and server are built from the same workspace.

### Development

Prerequisites: Rust toolchain (stable) and Node.js 18+.

```bash
# Clone
git clone https://github.com/mrpollo/flight-review-rs.git
cd flight-review-rs

# Build backend (debug)
cargo build

# Run the server locally with SQLite
cargo run -p flight-review-server -- serve \
  --db "sqlite://data/flight-review.db?mode=rwc" \
  --storage "file://data/files"

# Run the CLI
cargo run -p flight-review --bin ulog-convert -- --help
```

In a second terminal, start the frontend dev server:

```bash
cd frontend
npm install
npm run dev
```

The Vite dev server runs on `http://localhost:5173` and proxies all `/api` requests to the backend at `http://localhost:8080`. Open the Vite URL in the browser for development.

**Tests:**

```bash
# Backend
cargo test

# Frontend
cd frontend && npm test
```

**Type checking:**

```bash
cd frontend && npm run check
```

### Seeding Data

The `scripts/download-logs.sh` script downloads real ULog files from the v1 Flight Review instance at review.px4.io and optionally uploads them to a local v2 server. Useful for populating a development instance with realistic data.

```bash
# Download 50 logs (no upload)
COUNT=50 ./scripts/download-logs.sh

# Download 20 logs and upload to local server
COUNT=20 UPLOAD_URL=http://localhost:8080 ./scripts/download-logs.sh

# Upload previously downloaded logs only (skip download)
UPLOAD_ONLY=true UPLOAD_URL=http://localhost:8080 ./scripts/download-logs.sh
```

Key environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `COUNT` | `100` | Number of logs to download |
| `UPLOAD_URL` | (empty) | Server URL to upload to; empty skips upload |
| `UPLOAD_ONLY` | `false` | Skip downloading, upload existing files from output dir |
| `RATING_FILTER` | `good\|great` | Pipe-separated ratings to include; `none` for any |
| `GPS_ONLY` | `true` | Only download logs with GPS-dependent flight modes |
| `VERIFY` | `true` | Verify each file with `ulog-convert` before uploading |
| `MIN_VERSION` | `v1.14` | Minimum PX4 version |

### Release Build

```bash
cargo build --release
```

Build the frontend for production:

```bash
cd frontend && npm run build
```

This produces static files in `frontend/build/` that can be served by the backend or any static file server.

### Feature Flags

| Feature | Crate | Description | Default |
|---------|-------|-------------|---------|
| `sqlite` | server | SQLite database backend | Yes |
| `postgres` | server | PostgreSQL database backend | No |
| `s3` | server | Amazon S3 storage backend | No |

Build with specific features:

```bash
# With Postgres support
cargo build --release -p flight-review-server --features postgres

# With S3 support
cargo build --release -p flight-review-server --features s3

# With everything
cargo build --release -p flight-review-server --features "postgres,s3"
```

### Database Support

- **SQLite** (default) -- zero setup, single file, ideal for self-hosted
- **PostgreSQL** -- production deployments, concurrent access, managed hosting (AWS RDS, etc.)

Both backends auto-create the schema on startup.

### Storage Support

- **Local filesystem** (`file:///path`) -- simplest, no cloud needed
- **Amazon S3** (`s3://bucket/prefix`) -- production, scalable, integrates with CloudFront

## Deploy

The deployment spectrum ranges from a single self-contained binary on a Raspberry Pi to a production setup with CloudFront CDN, S3 storage, and managed Postgres. The same codebase supports all deployment models.

### Minimal (single binary)

Just run the binary with SQLite and local files -- no external services required:

```bash
./flight-review-server serve \
  --db sqlite:///data/flight-review.db \
  --storage file:///data/files \
  --port 8080

# Upload a log
curl -X POST http://localhost:8080/api/upload \
  -F "file=@flight.ulg" \
  -F "is_public=true" \
  -F "description=Test flight"

# List logs
curl http://localhost:8080/api/logs
```

### Docker

The Dockerfile currently builds the backend only. The frontend must be built separately (`cd frontend && npm run build`) and served via a reverse proxy or integrated into the container build.

```bash
# Build
docker build -t flight-review .

# Run with local storage
docker run -p 8080:8080 -v /data:/data flight-review
```

### Production (AWS)

Postgres for the database, S3 for file storage, and optionally CloudFront for CDN:

```bash
# Run with Postgres + S3
docker run -p 8080:8080 \
  flight-review serve \
  --db postgres://user:pass@host/flightreview \
  --storage s3://my-bucket/logs

# Full AWS example with credentials
docker run -p 8080:8080 \
  -e AWS_ACCESS_KEY_ID=... \
  -e AWS_SECRET_ACCESS_KEY=... \
  -e AWS_REGION=us-east-1 \
  flight-review serve \
  --db postgres://user:pass@rds-host.amazonaws.com/flightreview \
  --storage s3://px4-flight-review \
  --v1-ulg-prefix flight_review/log_files
```

## Migrate from v1

The migration tool imports metadata from a v1 Flight Review SQLite database into v2, preserving all UUIDs, delete tokens, and public/private flags. No log files are moved -- the original `.ulg` files stay in their existing storage location. Logs are converted to Parquet lazily on first view, or optionally in batch.

The migration extracts what it can from v1's `LogsGenerated` table (vehicle type from `MavType`, error/warning counts, vehicle UUID, software git hash). Fields that require parsing the `.ulg` file (vibration status, GPS quality, battery stats, localization sources, flight distance) remain unpopulated until the log is converted -- either lazily on first view or via batch conversion. Search and statistics results for these fields will be incomplete until conversion occurs.

### Metadata import + lazy conversion (recommended)

Import database records instantly. Logs are converted to Parquet on first view. No downtime, no batch job required.

```bash
# Import metadata from v1 SQLite
./flight-review-server migrate \
  --v1-db sqlite:///path/to/logs.sqlite \
  --db postgres://user:pass@host/flightreview

# Start server with lazy conversion (converts .ulg --> Parquet on first view)
./flight-review-server serve \
  --db postgres://user:pass@host/flightreview \
  --storage s3://px4-flight-review \
  --v1-ulg-prefix flight_review/log_files
```

### Metadata import + batch conversion (optional)

Import database records, then pre-convert all logs in the background. Useful for pre-warming cache or populating search indexes.

```bash
# Import metadata
./flight-review-server migrate \
  --v1-db sqlite:///path/to/logs.sqlite \
  --db postgres://user:pass@host/flightreview

# Batch-convert all pending logs
./flight-review-server convert-all \
  --db postgres://user:pass@host/flightreview \
  --storage s3://px4-flight-review \
  --v1-ulg-prefix flight_review/log_files
```

## CLI

`ulog-convert` is a standalone command-line tool for converting PX4 ULog files to Parquet and JSON. It can extract metadata, run flight analysis and diagnostics, perform PID step response analysis, and batch-scan directories for anomalies -- all without running the server or touching a database.

```bash
# Single file conversion (produces Parquet + metadata.json + manifest.json)
ulog-convert flight.ulg output_dir/

# Metadata + diagnostics only (JSON to stdout)
ulog-convert --metadata-only flight.ulg

# Compact JSON (for scripting)
ulog-convert --metadata-only --output-format compact flight.ulg | jq .

# Signal processing analysis
ulog-convert analyze flight.ulg
ulog-convert analyze flight.ulg -m pid_step_response

# Batch: convert a directory to Parquet (parallel, produces index.json)
ulog-convert batch logs/ -o dataset/

# Batch: scan for anomalies
ulog-convert batch logs/ --diagnostics-only

# Batch: convert + diagnose + analyze
ulog-convert batch logs/ -o dataset/ --diagnostics --analyze

# Batch: filter to specific analyzers
ulog-convert batch logs/ --diagnostics-only --analyzer gps_interference,ekf_failure

# JSON output for scripting
ulog-convert batch logs/ --diagnostics-only --format json
```

### Conversion Output

Every conversion produces a self-describing output directory:

```
output/
├── manifest.json              # what's here: source, topics, file map, diagnostics
├── metadata.json              # full flight metadata and analysis
├── vehicle_attitude.parquet   # one Parquet file per ULog topic
├── sensor_combined.parquet
└── ...
```

Batch conversions add an `index.json` at the output root:

```
dataset/
├── index.json                 # indexes all logs with manifest paths
├── sample/
│   ├── manifest.json
│   ├── metadata.json
│   └── *.parquet
├── motor_failure/
│   ├── manifest.json
│   └── ...
```

## Upload Context Fields

The upload endpoint accepts optional pilot-provided metadata as multipart form fields:

| Field | Type | Description |
|-------|------|-------------|
| `file` | file | The `.ulg` file (required) |
| `is_public` | bool | Show in public listings (default: false) |
| `description` | text | Flight description |
| `pilot_name` | text | Who flew |
| `vehicle_name` | text | Vehicle callsign |
| `tags` | text | Comma-separated labels |
| `rating` | int | Flight quality 1-5 |
| `wind_speed` | text | calm, breeze, gale, storm |
| `mission_type` | text | survey, inspection, test, recreational |
| `source` | text | web, CI, QGC, API |
| `feedback` | text | Pilot notes |
| `video_url` | text | Link to flight video |
| `location_name` | text | Human-readable location |

## Diagnostics

Flight Review automatically detects flight anomalies during upload. Diagnostic analyzers run inside the existing `analyze()` streaming pass -- no separate processing step, no background jobs. Results are stored in `metadata.json`, the `log_diagnostics` database table, and returned via the API.

### Available Analyzers

| Analyzer | Detects | Severity | Topics |
|----------|---------|----------|--------|
| `motor_failure` | PWM drop to zero or locked at max while armed | Critical/Warning | `actuator_outputs`, `vehicle_status` |
| `gps_interference` | EPH/EPV spikes, satellite count drops | Critical/Warning | `vehicle_gps_position` |
| `battery_brownout` | Voltage below critical threshold during flight | Critical | `battery_status`, `vehicle_status` |
| `ekf_failure` | Sustained EKF innovation test ratio exceedance | Critical/Warning | `estimator_status` |
| `rc_loss` | RC signal loss during armed flight | Critical/Warning | `input_rc`, `vehicle_status` |

Query logs by diagnostic:

```bash
# Logs with motor failures
curl "http://localhost:8080/api/logs?diagnostic=motor_failure"

# Logs with any critical diagnostic
curl "http://localhost:8080/api/logs?diagnostic_severity=critical"
```

### Adding a New Analyzer

See the full contributor guide at [`crates/converter/src/diagnostics/CONTRIBUTING.md`](crates/converter/src/diagnostics/CONTRIBUTING.md). It walks through the five steps (add an `Evidence` variant, create the analyzer file, register it, write the required tests, run the CI gates locally), includes a copy-pasteable skeleton, and points at [`rc_loss.rs`](crates/converter/src/diagnostics/rc_loss.rs) as the shortest complete reference implementation.

CI (`diagnostics.yml`) validates the pattern automatically on PRs that touch the diagnostics directory. Run `scripts/ci/check-analyzer.sh` locally to verify before pushing.

## For Researchers

There are two paths for working with flight log data, depending on your tools and workflow.

### Path 1: Parquet export for Python / ML workflows

Convert ULog files to Parquet and work with them using your existing tools (polars, pandas, DuckDB, scikit-learn, PyTorch). The CLI handles all the ULog parsing and produces a self-describing dataset.

```bash
# Convert a directory of flight logs to Parquet
ulog-convert batch logs/ -o dataset/ --diagnostics

# Output structure:
# dataset/
# ├── index.json              ← entry point: lists all logs
# ├── log_001/
# │   ├── manifest.json       ← file map + diagnostic labels
# │   ├── metadata.json       ← full flight metadata
# │   ├── vehicle_attitude.parquet
# │   ├── sensor_combined.parquet
# │   └── ...
# └── log_002/
#     └── ...
```

From Python:

```python
import json, polars as pl

# Load the dataset index
with open("dataset/index.json") as f:
    index = json.load(f)

# Find logs with motor failures
crashes = [log for log in index["logs"] if log["diagnostic_count"] > 0]

# Load a specific topic as a dataframe
df = pl.read_parquet(f"dataset/{crashes[0]['path']}/vehicle_attitude.parquet")
```

The diagnostic labels in each `manifest.json` provide pre-computed anomaly annotations with timestamps and severity -- usable as training labels for supervised learning without manually reviewing flights.

### Path 2: Rust-native signal processing modules

For analyses that need to run at scale across thousands of logs, or that you want to contribute back to the tool, write a Rust module that plugs into the signal processing framework. You declare what signals you need, the framework extracts them from the ULog file, and you receive buffered time-series data ready for FFT, spectral analysis, or deconvolution.

```bash
# Run signal processing on a single file
ulog-convert analyze flight.ulg

# Batch across a directory (parallel)
ulog-convert batch logs/ --analyze -m pid_step_response
```

#### Available Modules

| Module | Description | Topics |
|--------|-------------|--------|
| `pid_step_response` | PID controller step response via Wiener deconvolution | `vehicle_rates_setpoint`, `vehicle_angular_velocity` |

#### Adding a New Module

1. Create `crates/converter/src/signal_processing/your_module.rs`
2. Implement the `SignalAnalysis` trait (`id`, `description`, `required_signals`, `analyze`)
3. Register in `create_analyses()` in `signal_processing/mod.rs`
4. Use shared DSP utilities from `signal_processing/dsp.rs` (resampling, windowing, sample rate estimation)
5. Add tests following the pattern in `signal_processing/testing.rs`

Shared DSP functions available in `dsp.rs`: `median_sample_rate`, `resample_uniform`, `hanning_window`.

## Roadmap

- **User Accounts** -- optional authentication with email magic links, layered on top of the existing anonymous upload model
- **PID Analysis API** -- server-side endpoint exposing the existing PID step response analysis for frontend consumption
- **Dark Mode Polish** -- consistent dark mode across all pages (foundation exists but not fully polished)
- **Frontend Production Build** -- integrate static frontend build into Docker image and server binary

## License

MIT
