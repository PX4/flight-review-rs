#!/usr/bin/env bash
#
# Download ULog files from PX4 Flight Review v1 and optionally upload
# them to a flight-review-rs server instance.
#
# Modes:
#   (default)      Download + optional verify + optional upload
#   UPLOAD_ONLY=true  Skip downloading, upload existing files from OUTPUT_DIR
#
# Requirements: curl, jq
# Optional:     cargo (for --verify, builds ulog-convert if needed)
#
set -euo pipefail

# --- Configuration (override via env) ---
COUNT="${COUNT:-100}"
OUTPUT_DIR="${OUTPUT_DIR:-data/downloaded-logs}"
DBINFO_CACHE="${DBINFO_CACHE:-/tmp/dbinfo.json}"
DBINFO_URL="https://review.px4.io/dbinfo"

# Filters (set to "none" to disable rating filter, empty string uses default)
RATING_FILTER="${RATING_FILTER-good|great}"         # pipe-separated ratings, "none" = any
MIN_DURATION="${MIN_DURATION:-60}"                  # seconds
MAX_DURATION="${MAX_DURATION:-3600}"                # seconds
MAV_TYPE="${MAV_TYPE:-}"                            # e.g. "Quadrotor", empty = any
GPS_ONLY="${GPS_ONLY:-true}"                        # require GPS-dependent flight modes
MIN_VERSION="${MIN_VERSION:-v1.14}"                 # minimum PX4 version, empty = any
MAX_VERSION="${MAX_VERSION:-v2.0}"                  # max version (excludes custom builds like v50.x)
VERIFY="${VERIFY:-true}"                            # verify each file with ulog-convert

# Upload settings
UPLOAD_URL="${UPLOAD_URL:-}"                        # e.g. "http://localhost:8080", empty = skip upload
UPLOAD_ONLY="${UPLOAD_ONLY:-false}"                 # skip download, just upload existing files
UPLOAD_PUBLIC="${UPLOAD_PUBLIC:-true}"               # mark uploads as public

# --- Helpers ---
info()  { printf '\033[1;34m[INFO]\033[0m  %s\n' "$*"; }
warn()  { printf '\033[1;33m[WARN]\033[0m  %s\n' "$*"; }
error() { printf '\033[1;31m[ERROR]\033[0m %s\n' "$*" >&2; exit 1; }

for cmd in curl jq; do
  command -v "$cmd" &>/dev/null || error "$cmd is required but not found"
done

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- Upload function ---
upload_file() {
  local file="$1"
  local filename
  filename=$(basename "$file")

  if ! curl -fsS -X POST "${UPLOAD_URL}/api/upload" \
    -F "file=@${file}" \
    -F "is_public=${UPLOAD_PUBLIC}" \
    -F "description=Imported from Flight Review v1" \
    -F "source=flight-review-v1" \
    -o /dev/null -w "%{http_code}" 2>/dev/null | grep -q "200"; then
    return 1
  fi
  return 0
}

# --- Upload-only mode ---
if [[ "$UPLOAD_ONLY" == "true" ]]; then
  [[ -n "$UPLOAD_URL" ]] || error "UPLOAD_URL is required for upload-only mode"
  [[ -d "$OUTPUT_DIR" ]] || error "Output directory $OUTPUT_DIR does not exist"

  files=("$OUTPUT_DIR"/*.ulg)
  total=${#files[@]}
  [[ $total -gt 0 ]] || error "No .ulg files found in $OUTPUT_DIR"

  info "Uploading $total logs from $OUTPUT_DIR to $UPLOAD_URL ..."

  uploaded=0
  failed=0
  for file in "${files[@]}"; do
    uploaded=$((uploaded + 1))
    name=$(basename "$file" .ulg)
    if upload_file "$file"; then
      info "[$uploaded/$total] Uploaded ${name:0:8}..."
    else
      warn "[$uploaded/$total] FAILED ${name:0:8}..."
      failed=$((failed + 1))
    fi
  done

  info "=== Upload Summary ==="
  info "  Uploaded: $((uploaded - failed))/$total"
  [[ $failed -eq 0 ]] || warn "  Failed: $failed"
  exit 0
fi

# Build ulog-convert if verification is enabled
ULOG_CONVERT=""
if [[ "$VERIFY" == "true" ]]; then
  ULOG_CONVERT="$REPO_ROOT/target/release/ulog-convert"
  if [[ ! -x "$ULOG_CONVERT" ]]; then
    info "Building ulog-convert for verification..."
    (cd "$REPO_ROOT" && cargo build -p flight-review --bin ulog-convert --release 2>&1 | tail -1)
  fi
  [[ -x "$ULOG_CONVERT" ]] || error "Failed to build ulog-convert"
  info "Verification enabled: $ULOG_CONVERT"
fi

# --- Step 1: Fetch or reuse cached dbinfo ---
if [[ -f "$DBINFO_CACHE" ]]; then
  age=$(( $(date +%s) - $(stat -f %m "$DBINFO_CACHE" 2>/dev/null || stat -c %Y "$DBINFO_CACHE" 2>/dev/null) ))
  if (( age < 86400 )); then
    info "Using cached dbinfo.json (${age}s old)"
  else
    info "Cache is stale (${age}s), re-downloading..."
    rm -f "$DBINFO_CACHE"
  fi
fi

if [[ ! -f "$DBINFO_CACHE" ]]; then
  info "Downloading dbinfo.json from $DBINFO_URL ..."
  curl -fSL --compressed -o "$DBINFO_CACHE" "$DBINFO_URL"
  info "Saved to $DBINFO_CACHE ($(du -h "$DBINFO_CACHE" | cut -f1))"
fi

# --- Step 2: Build jq filter and select logs ---
info "Filtering logs (rating=[${RATING_FILTER:-any}], duration=${MIN_DURATION}-${MAX_DURATION}s, mav_type=${MAV_TYPE:-any}, gps=$GPS_ONLY, ver=${MIN_VERSION:-any}-${MAX_VERSION:-any})..."

JQ_FILTER='
  [.[] | select(
    (.download_url // "") != ""
    and (.duration_s // 0) >= ($min_dur | tonumber)
    and (.duration_s // 0) <= ($max_dur | tonumber)
  )]
'

# Rating filter ("none" or empty = skip)
if [[ -n "$RATING_FILTER" && "$RATING_FILTER" != "none" ]]; then
  JQ_FILTER="$JQ_FILTER"' | [.[] | select(.rating | test($rating; "i"))]'
fi

# Vehicle type filter
if [[ -n "$MAV_TYPE" ]]; then
  JQ_FILTER="$JQ_FILTER"' | [.[] | select(.mav_type == $mav_type)]'
fi

# GPS filter: require at least one GPS-dependent flight mode
# PX4 nav_state: 2=Position, 3=Mission, 4=Loiter, 5=RTL, 12=Descend, 13=Orbit, 21=Takeoff
if [[ "$GPS_ONLY" == "true" ]]; then
  JQ_FILTER="$JQ_FILTER"' | [.[] | select(
    .flight_modes as $fm |
    [2,3,4,5,12,13,21] | any(. as $gps | $fm | index($gps) != null)
  )]'
fi

# Version filter: require ver_sw_release >= MIN_VERSION
if [[ -n "$MIN_VERSION" ]]; then
  JQ_FILTER="$JQ_FILTER"' | [.[] |
    select((.ver_sw_release // "") | test("^v[0-9]")) |
    select(
      (.ver_sw_release | capture("v(?<maj>[0-9]+)\\.(?<min>[0-9]+)")) as $v |
      ($min_ver | capture("v(?<maj>[0-9]+)\\.(?<min>[0-9]+)")) as $req |
      (($v.maj | tonumber) > ($req.maj | tonumber) or
       (($v.maj | tonumber) == ($req.maj | tonumber) and ($v.min | tonumber) >= ($req.min | tonumber)))
    )]'
fi

# Max version filter: exclude custom builds with unrealistic version numbers
if [[ -n "$MAX_VERSION" ]]; then
  JQ_FILTER="$JQ_FILTER"' | [.[] |
    select(
      (.ver_sw_release | capture("v(?<maj>[0-9]+)\\.(?<min>[0-9]+)")) as $v |
      ($max_ver | capture("v(?<maj>[0-9]+)\\.(?<min>[0-9]+)")) as $cap |
      (($v.maj | tonumber) < ($cap.maj | tonumber) or
       (($v.maj | tonumber) == ($cap.maj | tonumber) and ($v.min | tonumber) <= ($cap.min | tonumber)))
    )]'
fi

# Take N (deterministic order by log_id)
JQ_FILTER="$JQ_FILTER"' | sort_by(.log_id) | .[:($count | tonumber)]'

SELECTED=$(jq -r \
  --arg min_dur "$MIN_DURATION" \
  --arg max_dur "$MAX_DURATION" \
  --arg rating "${RATING_FILTER:-^$}" \
  --arg mav_type "$MAV_TYPE" \
  --arg min_ver "${MIN_VERSION:-v0.0}" \
  --arg max_ver "${MAX_VERSION:-v99.99}" \
  --arg count "$COUNT" \
  "$JQ_FILTER" "$DBINFO_CACHE")

TOTAL=$(echo "$SELECTED" | jq 'length')
info "Found $TOTAL logs matching filters (requested $COUNT)"

if (( TOTAL == 0 )); then
  error "No logs matched the filters. Try relaxing RATING_FILTER, MIN_VERSION, or duration range."
fi

# --- Step 3: Download and optionally verify ---
mkdir -p "$OUTPUT_DIR"

echo "$SELECTED" | jq -r '.[] | "\(.log_id)\t\(.download_url)\t\(.mav_type)\t\(.rating)\t\(.duration_s)\t\(.ver_sw_release)"' | \
{
  i=0
  failed=0
  invalid=0
  skipped=0
  upload_ok=0
  upload_fail=0
  while IFS=$'\t' read -r log_id url mav_type rating duration version; do
    i=$((i + 1))
    dest="$OUTPUT_DIR/${log_id}.ulg"

    if [[ -f "$dest" ]]; then
      info "[$i/$TOTAL] Already exists: $log_id"
      skipped=$((skipped + 1))
      # Still upload existing files if configured
      if [[ -n "$UPLOAD_URL" ]]; then
        if upload_file "$dest"; then
          info "  -> uploaded to $UPLOAD_URL"
          upload_ok=$((upload_ok + 1))
        else
          warn "  -> upload FAILED"
          upload_fail=$((upload_fail + 1))
        fi
      fi
      continue
    fi

    info "[$i/$TOTAL] Downloading $log_id ($mav_type, $rating, ${duration}s, $version)..."
    if ! curl -fSL --compressed -o "$dest" "$url" 2>/dev/null; then
      warn "  -> DOWNLOAD FAILED, skipping"
      rm -f "$dest"
      failed=$((failed + 1))
      continue
    fi

    size=$(du -h "$dest" | cut -f1)

    # Verify with ulog-convert
    if [[ -n "$ULOG_CONVERT" ]]; then
      if $ULOG_CONVERT --metadata-only "$dest" > /dev/null 2>&1; then
        info "  -> $size (verified)"
      else
        warn "  -> $size (INVALID ULog, removing)"
        rm -f "$dest"
        invalid=$((invalid + 1))
        continue
      fi
    else
      info "  -> $size"
    fi

    # Upload if configured
    if [[ -n "$UPLOAD_URL" ]]; then
      if upload_file "$dest"; then
        info "  -> uploaded to $UPLOAD_URL"
        upload_ok=$((upload_ok + 1))
      else
        warn "  -> upload FAILED"
        upload_fail=$((upload_fail + 1))
      fi
    fi
  done

  downloaded=$((i - failed - invalid - skipped))
  info "=== Summary ==="
  info "  Downloaded: $downloaded"
  info "  Skipped (existing): $skipped"
  [[ $failed -eq 0 ]]  || warn "  Failed downloads: $failed"
  [[ $invalid -eq 0 ]] || warn "  Invalid ULog files: $invalid"
  if [[ -n "$UPLOAD_URL" ]]; then
    info "  Uploaded: $upload_ok"
    [[ $upload_fail -eq 0 ]] || warn "  Upload failures: $upload_fail"
  fi
  info "  Output: $OUTPUT_DIR/"
}
