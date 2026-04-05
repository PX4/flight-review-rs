#!/usr/bin/env bash
#
# Check that conversion benchmark is within the performance budget.
# Parses criterion output for the mean time and compares against
# the budget for the file size tier.
#
# Budget (from architecture doc):
#   < 10 MB:   500 ms
#   10-100 MB: 2000 ms
#   100-512 MB: 10000 ms
#
# Usage: scripts/ci/check-bench-budget.sh <bench-output.txt>

set -euo pipefail

BENCH_FILE="${1:?Usage: check-bench-budget.sh <bench-output.txt>}"

if [[ ! -f "$BENCH_FILE" ]]; then
    echo "ERROR: Benchmark output file not found: $BENCH_FILE"
    exit 1
fi

echo "Parsing benchmark output..."
cat "$BENCH_FILE"
echo

# Criterion default output looks like:
#   convert_ulog_sample     time:   [36.749 ms 37.071 ms 37.534 ms]
# We want the middle value (mean estimate).
mean_line=$(grep 'time:' "$BENCH_FILE" | head -1 || true)

if [[ -z "$mean_line" ]]; then
    echo "ERROR: Could not find 'time:' line in benchmark output."
    echo "This likely means the benchmark failed to run."
    exit 1
fi

echo "Found: $mean_line"

# Extract the middle value and unit
# Format: time:   [36.749 ms 37.071 ms 37.534 ms]
mean_val=$(echo "$mean_line" | sed -E 's/.*\[.*[[:space:]]([0-9.]+)[[:space:]]+(ms|s|us|ns).*/\1/')
mean_unit=$(echo "$mean_line" | sed -E 's/.*\[.*[[:space:]][0-9.]+[[:space:]]+(ms|s|us|ns).*/\1/')

if [[ -z "$mean_val" || -z "$mean_unit" ]]; then
    echo "ERROR: Could not parse mean time from: $mean_line"
    exit 1
fi

# Convert to milliseconds
case "$mean_unit" in
    ns) time_ms=$(echo "$mean_val / 1000000" | bc -l) ;;
    us) time_ms=$(echo "$mean_val / 1000" | bc -l) ;;
    ms) time_ms="$mean_val" ;;
    s)  time_ms=$(echo "$mean_val * 1000" | bc -l) ;;
    *)  echo "ERROR: Unknown unit: $mean_unit"; exit 1 ;;
esac

# Budget: sample.ulg is ~4MB, budget is 500ms for <10MB files
budget_ms=500
echo
printf "Mean time:  %.1f ms\n" "$time_ms"
printf "Budget:     %d ms (< 10 MB tier)\n" "$budget_ms"

if (( $(echo "$time_ms > $budget_ms" | bc -l) )); then
    echo "FAIL: Conversion exceeded ${budget_ms}ms budget"
    exit 1
fi

echo "PASS: Within performance budget"
