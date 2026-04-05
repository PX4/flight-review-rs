#!/usr/bin/env bash
#
# Check conversion benchmark against absolute budget and baseline regression.
#
# Two checks:
#   1. Absolute budget: must stay under 500ms for <10MB files
#   2. Regression: must not regress >10% vs cached baseline from main
#
# The regression check only runs when criterion has a cached baseline
# (target/criterion/ from a previous run). On first run or cold cache
# it gracefully skips the regression check.
#
# Usage: scripts/ci/check-bench-budget.sh <bench-output.txt>

set -euo pipefail

BENCH_FILE="${1:?Usage: check-bench-budget.sh <bench-output.txt>}"
MAX_REGRESSION_PCT=10

if [[ ! -f "$BENCH_FILE" ]]; then
    echo "ERROR: Benchmark output file not found: $BENCH_FILE"
    exit 1
fi

# --- Parse mean time ---

# Criterion output: time:   [36.749 ms 37.071 ms 37.534 ms]
mean_line=$(grep 'time:' "$BENCH_FILE" | head -1 || true)

if [[ -z "$mean_line" ]]; then
    echo "ERROR: Could not find 'time:' line in benchmark output."
    echo "This likely means the benchmark failed to run."
    exit 1
fi

# Extract the middle value (mean estimate) and unit
mean_val=$(echo "$mean_line" | sed -E 's/.*\[.*[[:space:]]([0-9.]+)[[:space:]]+(ms|s|us|ns).*/\1/')
mean_unit=$(echo "$mean_line" | sed -E 's/.*\[.*[[:space:]][0-9.]+[[:space:]]+(ms|s|us|ns).*/\1/')

if [[ -z "$mean_val" || -z "$mean_unit" ]]; then
    echo "ERROR: Could not parse mean time from: $mean_line"
    exit 1
fi

case "$mean_unit" in
    ns) time_ms=$(echo "$mean_val / 1000000" | bc -l) ;;
    us) time_ms=$(echo "$mean_val / 1000" | bc -l) ;;
    ms) time_ms="$mean_val" ;;
    s)  time_ms=$(echo "$mean_val * 1000" | bc -l) ;;
    *)  echo "ERROR: Unknown unit: $mean_unit"; exit 1 ;;
esac

# --- Check 1: Absolute budget ---

budget_ms=500
printf "Mean time:  %.1f ms\n" "$time_ms"
printf "Budget:     %d ms (< 10 MB tier)\n" "$budget_ms"

if (( $(echo "$time_ms > $budget_ms" | bc -l) )); then
    echo "FAIL: Conversion exceeded ${budget_ms}ms budget"
    exit 1
fi
echo "PASS: Within absolute budget"

# --- Check 2: Regression vs baseline ---

# Criterion prints: change: [-1.34% +0.19% +1.60%] (p = 0.83 > 0.05)
# Or:               change: [+5.2% +7.1% +9.0%] (p = 0.02 < 0.05)
#                   Performance has regressed.
change_line=$(grep 'change:' "$BENCH_FILE" | head -1 || true)

if [[ -z "$change_line" ]]; then
    echo "No baseline available for regression check (first run or cold cache)"
    echo "SKIP: Regression check"
    exit 0
fi

# Extract the middle percentage (mean change estimate)
change_pct=$(echo "$change_line" | sed -E 's/.*change:[[:space:]]*\[.*[[:space:]]([+-]?[0-9.]+)%.*/\1/')

if [[ -z "$change_pct" ]]; then
    echo "Could not parse change percentage from: $change_line"
    echo "SKIP: Regression check"
    exit 0
fi

printf "Change:     %s%% vs baseline\n" "$change_pct"
printf "Threshold:  +%d%%\n" "$MAX_REGRESSION_PCT"

# Check if the change exceeds the threshold (positive = regression)
is_regression=$(echo "$change_pct > $MAX_REGRESSION_PCT" | bc -l 2>/dev/null || echo 0)

if [[ "$is_regression" == "1" ]]; then
    echo "FAIL: Performance regressed by ${change_pct}% (threshold: +${MAX_REGRESSION_PCT}%)"
    exit 1
fi

echo "PASS: No significant regression"
