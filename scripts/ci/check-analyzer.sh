#!/usr/bin/env bash
#
# Validates that diagnostic analyzers follow the required contributor pattern.
# Called by CI on PRs that touch crates/converter/src/diagnostics/.
#
# Checks for each analyzer file:
#   1. Implements id() and description() on the Analyzer trait
#   2. Has a corresponding test fixture .ulg in tests/fixtures/
#   3. Has required test categories:
#      - no_false_positives (sample.ulg)
#      - real-world fixture test (detects_real_*)
#      - handles_missing_fields
#      - snapshot test
#   4. Is registered in create_analyzers() factory
#   5. Has an Evidence enum variant
#   6. Implements output_descriptor()
#
# Usage: scripts/ci/check-analyzer.sh [--changed-only]
#   --changed-only: only check analyzers modified in the current PR

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DIAG_DIR="$REPO_ROOT/crates/converter/src/diagnostics"
FIXTURE_DIR="$REPO_ROOT/crates/converter/tests/fixtures"
MOD_FILE="$DIAG_DIR/mod.rs"

errors=0
warnings=0

err() { echo "ERROR: $1"; errors=$((errors + 1)); }
warn() { echo "WARN:  $1"; warnings=$((warnings + 1)); }
ok() { echo "OK:    $1"; }

# Determine which analyzer files to check
if [[ "${1:-}" == "--changed-only" ]]; then
    # Get files changed in this PR vs main
    changed_files=$(git diff --name-only origin/main...HEAD -- "$DIAG_DIR/" 2>/dev/null || \
                    git diff --name-only HEAD~1 -- "$DIAG_DIR/" 2>/dev/null || echo "")

    # Extract unique analyzer names from changed paths.
    # `grep -v` exits 1 when nothing matches (e.g. only a snapshot or fixture
    # changed, so sed produced no analyzer names); under `set -e` + `pipefail`
    # that would abort the whole script before the "nothing changed" check
    # below. `|| true` keeps an empty result from failing the pipeline.
    analyzer_names=$(echo "$changed_files" \
        | sed -n 's|^crates/converter/src/diagnostics/\([^/.]*\)\.rs$|\1|p' \
        | { grep -v -E '^(mod|testing)$' || true; } \
        | sort -u)

    analyzer_files=""
    for name in $analyzer_names; do
        [[ -f "$DIAG_DIR/${name}.rs" ]] && analyzer_files="$analyzer_files $DIAG_DIR/${name}.rs"
    done

    if [[ -z "$analyzer_files" ]]; then
        echo "No analyzer files changed. Skipping checks."
        exit 0
    fi
else
    # Find all analyzer .rs files (excluding mod.rs, testing.rs)
    analyzer_files=$(find "$DIAG_DIR" -maxdepth 1 -name "*.rs" \
        ! -name "mod.rs" \
        ! -name "testing.rs" \
        | sort)
fi

echo "Checking diagnostic analyzers..."
echo

for analyzer_file in $analyzer_files; do
    name=$(basename "$analyzer_file" .rs)
    echo "--- $name ---"

    # 1. Check id() and description() are implemented
    if grep -q 'fn id(&self)' "$analyzer_file"; then
        ok "implements id()"
    else
        err "$name: missing fn id() implementation"
    fi

    if grep -q 'fn description(&self)' "$analyzer_file"; then
        ok "implements description()"
    else
        err "$name: missing fn description() implementation"
    fi

    # 2. Check for real-world test fixture
    # Analyzers may skip the fixture requirement if they document why
    # in a SKIP_FIXTURE comment (e.g. no known ULog exhibits the failure)
    fixture="$FIXTURE_DIR/${name}.ulg"
    if [[ -f "$fixture" ]]; then
        size=$(du -h "$fixture" | cut -f1)
        ok "test fixture exists ($size)"
    elif grep -q 'SKIP_FIXTURE' "$analyzer_file"; then
        warn "$name: no fixture (SKIP_FIXTURE documented in source)"
    else
        err "$name: missing test fixture at tests/fixtures/${name}.ulg"
    fi

    # 3. Check required test categories
    if grep -q 'no_false_positives' "$analyzer_file"; then
        ok "has no_false_positives test"
    else
        err "$name: missing no_false_positives test"
    fi

    if grep -q 'detects_real_' "$analyzer_file"; then
        ok "has real-world fixture test"
    elif grep -q 'SKIP_FIXTURE' "$analyzer_file"; then
        warn "$name: no real-world test (SKIP_FIXTURE documented)"
    else
        err "$name: missing real-world fixture test (detects_real_*)"
    fi

    if grep -q 'handles_missing_fields' "$analyzer_file"; then
        ok "has missing fields test"
    else
        err "$name: missing handles_missing_fields test"
    fi

    if grep -q 'assert_json_snapshot' "$analyzer_file"; then
        ok "has snapshot test"
    else
        err "$name: missing insta snapshot test"
    fi

    # 4. Check registered in create_analyzers()
    struct_name=$(grep -o 'pub struct [A-Za-z]*' "$analyzer_file" | head -1 | awk '{print $3}')
    if [[ -n "$struct_name" ]] && grep -q "$struct_name" "$MOD_FILE"; then
        ok "registered in create_analyzers()"
    else
        err "$name: not registered in create_analyzers() in mod.rs"
    fi

    # 5. Check Evidence enum variant exists
    if grep -q "evidence: Evidence::" "$analyzer_file"; then
        ok "uses Evidence enum variant"
    else
        err "$name: missing Evidence enum variant usage"
    fi

    # 6. Check output_descriptor() is implemented
    if grep -q 'fn output_descriptor' "$analyzer_file"; then
        ok "implements output_descriptor()"
    else
        err "$name: missing fn output_descriptor() implementation"
    fi

    echo
done

# 7. Check that all analyzer modules are declared in mod.rs
echo "--- mod.rs declarations ---"
for analyzer_file in $analyzer_files; do
    name=$(basename "$analyzer_file" .rs)
    if grep -q "pub mod ${name};" "$MOD_FILE"; then
        ok "$name declared in mod.rs"
    else
        err "$name: not declared as 'pub mod ${name};' in mod.rs"
    fi
done

echo
echo "=============================="
echo "Results: $errors errors, $warnings warnings"

if [[ $errors -gt 0 ]]; then
    echo
    echo "Analyzer validation failed. See errors above."
    echo "Refer to crates/converter/src/diagnostics/testing.rs for requirements."
    exit 1
fi

exit 0
