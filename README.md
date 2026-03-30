# Flight Review v2

Flight Review v2 is a complete rewrite of [PX4 Flight Review](https://github.com/PX4/flight_review) in Rust. It replaces the "parse every time you view" model with a **parse-once-store-review** architecture: ULog files are converted to per-topic Parquet files and a rich metadata JSON at upload time, then served as static files for client-side analysis via DuckDB-WASM. The result is sub-second log viewing with zero server-side compute, support for SQLite or Postgres, and local filesystem or S3 storage — deployable as a single binary or a Docker container.

## Architecture

```
Upload .ulg ──► Rust converter ──► Per-topic Parquet + metadata.json ──► Storage (local / S3)
                                                                              │
Browser ◄── DuckDB-WASM queries Parquet via HTTP Range requests ◄─────────────┘
```

### Workspace layout

```
flight-review-rs/
├── crates/
│   ├── converter/          # Library + CLI
│   │   ├── metadata.rs     # All 13 ULog message types → metadata.json
│   │   ├── analysis.rs     # Flight modes, stats, battery, GPS, vibration, param diff
│   │   ├── pid_analysis.rs # Wiener deconvolution step response (roll/pitch/yaw)
│   │   ├── converter.rs    # ULog → per-topic ZSTD Parquet files
│   │   └── bin/
│   │       └── ulog_convert.rs
│   └── server/             # HTTP API server
│       ├── api/            # Upload, list, get, delete, file serving (Range requests)
│       ├── db/             # LogStore trait — SQLite and Postgres backends
│       └── storage/        # object_store — local filesystem and S3
├── Dockerfile
└── README.md
```

### What gets stored per log

All files live under a single UUID directory:

```
<uuid>/
├── metadata.json           # Metadata + flight analysis (modes, stats, vibration, GPS track, params)
├── <uuid>.ulg              # Original upload
├── vehicle_attitude.parquet
├── sensor_combined.parquet
├── battery_status.parquet
└── ...                     # One Parquet file per ULog topic
```

### API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/health` | Health check |
| `POST` | `/api/upload` | Multipart upload — accepts `.ulg` file + optional context fields |
| `GET` | `/api/logs` | List/search logs (paginated, filtered by hardware, public/private, etc.) |
| `GET` | `/api/logs/:id` | Single log record |
| `DELETE` | `/api/logs/:id?token=<token>` | Delete log (requires delete token from upload) |
| `GET` | `/api/logs/:id/data/:filename` | Serve Parquet/JSON/ULG files with HTTP Range support |

### Database

Feature-flagged at compile time:

- **SQLite** (default) — zero setup, good for self-hosted and small deployments
- **Postgres** — for production deployments with many users

Both backends auto-create the schema on startup.

### Storage

Configured at runtime via URL:

- `file:///data/files` — local filesystem
- `s3://bucket-name/prefix` — Amazon S3 (requires `s3` feature flag)

## Build

### Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs))

### From source

```bash
# Build both binaries
cargo build --release

# Binaries are at:
#   target/release/ulog-convert
#   target/release/flight-review-server
```

### With Postgres support

```bash
cargo build --release -p flight-review-server --features postgres
```

### With S3 support

```bash
cargo build --release -p flight-review-server --features s3
```

## Deploy

### Quickstart (local)

```bash
# Start the server with SQLite + local storage
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

```bash
# Build
docker build -t flight-review .

# Run with local storage
docker run -p 8080:8080 -v /data:/data flight-review

# Run with Postgres + S3
docker run -p 8080:8080 \
  flight-review serve \
  --db postgres://user:pass@host/flightreview \
  --storage s3://my-bucket/logs
```

### Production (AWS)

```bash
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

```bash
# Import metadata from v1 SQLite (preserves UUIDs, tokens, public flags)
./flight-review-server migrate \
  --v1-db sqlite:///path/to/logs.sqlite \
  --db postgres://user:pass@host/flightreview

# Start server with lazy conversion (converts .ulg → Parquet on first view)
./flight-review-server serve \
  --db postgres://user:pass@host/flightreview \
  --storage s3://px4-flight-review \
  --v1-ulg-prefix flight_review/log_files
```

## CLI

The `ulog-convert` binary is a standalone conversion tool:

```bash
# Full conversion (Parquet + metadata.json)
ulog-convert flight.ulg output_dir/

# Metadata only (JSON to stdout, pipeable)
ulog-convert --metadata-only flight.ulg

# PID step response analysis
ulog-convert --pid-analysis flight.ulg

# Compact JSON (for scripting)
ulog-convert --metadata-only --output-format compact flight.ulg | jq .
```

## Upload context fields

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

## License

MIT
