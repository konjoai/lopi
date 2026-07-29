#!/usr/bin/env bash
# Kill-test for indexing_floor_check.py — Sprint S13R, Phase D.
#
# Proves, using a real fixture file planted under crates/lopi-core/src/ and cleaned up
# on exit (same technique as test_scope_assert_killtest.sh and
# test_function_length_killtest.sh, since this script's REPO_ROOT is fixed):
#   1. Test/bench files and comment-only lines are excluded from the count.
#   2. The --ceiling-file ratchet: a count at the locked ceiling passes; a count one
#      over it fails, naming the regression.
#
# Usage: bash .konjo/scripts/test_indexing_floor_killtest.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECK="$REPO_ROOT/.konjo/scripts/indexing_floor_check.py"
FIXTURE_DIR="$REPO_ROOT/crates/lopi-core/src/_indexing_floor_killtest_fixture"
TMP="$(mktemp -d)"
cleanup() { rm -rf "$TMP" "$FIXTURE_DIR"; }
trap cleanup EXIT

PASS=0
FAIL=0

echo "── Indexing floor kill-test ──"

BASELINE=$(grep -vE '^\s*#|^\s*$' "$REPO_ROOT/.konjo/indexing-floor.txt" | head -1)

mkdir -p "$FIXTURE_DIR"
{
  echo "// a comment mentioning [0] and [1] must not count"
  echo "fn _killtest_indexing(v: &[i32]) -> i32 {"
  echo "    v[0] + v[1]"  # 2 real sites
  echo "}"
} > "$FIXTURE_DIR/production.rs"

{
  echo "fn _killtest_indexing_in_test(v: &[i32]) -> i32 {"
  echo "    v[0] + v[1]"
  echo "}"
} > "$FIXTURE_DIR/fixture_tests.rs"

OUT=$(python3 "$CHECK")
COUNT=$(echo "$OUT" | grep -oE '^indexing_floor: [0-9]+' | grep -oE '[0-9]+')
EXPECTED=$((BASELINE + 2))

if [ "$COUNT" -eq "$EXPECTED" ]; then
  echo "PASS: comment + tests.rs sites excluded, only the 2 real production sites counted (baseline $BASELINE + 2 = $COUNT)"
  PASS=$((PASS + 1))
else
  echo "FAIL: expected $EXPECTED (baseline $BASELINE + 2 real sites), got $COUNT"
  echo "$OUT"
  FAIL=$((FAIL + 1))
fi

echo "$EXPECTED" > "$TMP/ceiling_ok.txt"
echo "$((EXPECTED - 1))" > "$TMP/ceiling_too_low.txt"

set +e
python3 "$CHECK" --ceiling-file "$TMP/ceiling_ok.txt" >"$TMP/ok.log" 2>&1
OK_EXIT=$?
python3 "$CHECK" --ceiling-file "$TMP/ceiling_too_low.txt" >"$TMP/low.log" 2>&1
LOW_EXIT=$?
set -e

if [ "$OK_EXIT" -eq 0 ]; then
  echo "PASS: count at the locked ceiling passes (exit 0)"
  PASS=$((PASS + 1))
else
  echo "FAIL: expected exit 0 at the locked ceiling, got $OK_EXIT"
  cat "$TMP/ok.log"
  FAIL=$((FAIL + 1))
fi

if [ "$LOW_EXIT" -eq 1 ]; then
  echo "PASS: count one over the locked ceiling fails (exit 1)"
  PASS=$((PASS + 1))
else
  echo "FAIL: expected exit 1 one over the locked ceiling, got $LOW_EXIT"
  cat "$TMP/low.log"
  FAIL=$((FAIL + 1))
fi

echo
echo "── Results: $PASS passed, $FAIL failed ──"
[ "$FAIL" -eq 0 ]
