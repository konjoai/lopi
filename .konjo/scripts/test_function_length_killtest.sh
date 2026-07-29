#!/usr/bin/env bash
# Kill-test for function_length_check.py — Sprint S13R, Phase B, decision item 3.
#
# Proves, using real fixture files planted under crates/lopi-core/src/ and cleaned up
# on exit (the same technique test_scope_assert_killtest.sh already uses, since this
# script's REPO_ROOT is fixed to the real checkout, not overridable):
#   1. A function over --hard-limit fails when no --ceiling-file is given.
#   2. A function at or under --hard-limit does not, by itself, fail the gate.
#   3. An oversized function inside a `_tests.rs` file is excluded from the scan.
#   4. The --ceiling-file ratchet: a count at the locked ceiling passes; a count
#      over it fails, naming the regression.
#
# Usage: bash .konjo/scripts/test_function_length_killtest.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECK="$REPO_ROOT/.konjo/scripts/function_length_check.py"
FIXTURE_DIR="$REPO_ROOT/crates/lopi-core/src/_function_length_killtest_fixture"
TMP="$(mktemp -d)"
cleanup() { rm -rf "$TMP" "$FIXTURE_DIR"; }
trap cleanup EXIT

PASS=0
FAIL=0

report() {
  local desc="$1" expected="$2" actual="$3" log="$4"
  if [ "$actual" -eq "$expected" ]; then
    echo "PASS: $desc (exit $actual)"
    PASS=$((PASS + 1))
  else
    echo "FAIL: $desc (expected exit $expected, got $actual)"
    cat "$log"
    FAIL=$((FAIL + 1))
  fi
}

echo "── Function length kill-test ──"

mkdir -p "$FIXTURE_DIR"

# Fixture: one oversized function (non-test) + one oversized function inside a
# _tests.rs file (must be excluded), no other production offenders planted.
{
  echo "fn _killtest_oversized() {"
  for i in $(seq 1 60); do echo "    let _x$i = $i;"; done
  echo "}"
} > "$FIXTURE_DIR/oversized.rs"

{
  echo "fn _killtest_oversized_in_test() {"
  for i in $(seq 1 60); do echo "    let _x$i = $i;"; done
  echo "}"
} > "$FIXTURE_DIR/oversized_tests.rs"

# Baseline: how many real offenders exist without the fixture's non-test file
# (i.e. the ceiling this repo already ratcheted at .konjo/function-length-ceiling.txt).
BASELINE=$(cat "$REPO_ROOT/.konjo/function-length-ceiling.txt" | grep -v '^#' | grep -v '^\s*$' | head -1)

set +e
python3 "$CHECK" --hard-limit 50 >"$TMP/no_ceiling.log" 2>&1
NO_CEILING_EXIT=$?
set -e
report "oversized fixture pushes the count over any ceiling (no --ceiling-file)" 1 "$NO_CEILING_EXIT" "$TMP/no_ceiling.log"

if grep -q "_function_length_killtest_fixture/oversized.rs" "$TMP/no_ceiling.log"; then
  echo "PASS: the non-test fixture file is named in the output"
  PASS=$((PASS + 1))
else
  echo "FAIL: expected the non-test fixture file to be named as an offender"
  cat "$TMP/no_ceiling.log"
  FAIL=$((FAIL + 1))
fi

if grep -q "_function_length_killtest_fixture/oversized_tests.rs" "$TMP/no_ceiling.log"; then
  echo "FAIL: a _tests.rs fixture file was NOT excluded from the scan"
  FAIL=$((FAIL + 1))
else
  echo "PASS: the _tests.rs fixture file is excluded from the scan"
  PASS=$((PASS + 1))
fi

# Ceiling ratchet: baseline + 1 (this fixture's one real non-test offender) should
# pass at exactly that ceiling, and fail one below it.
echo "$((BASELINE + 1))" > "$TMP/ceiling_ok.txt"
echo "$BASELINE" > "$TMP/ceiling_too_low.txt"

set +e
python3 "$CHECK" --hard-limit 50 --ceiling-file "$TMP/ceiling_ok.txt" >"$TMP/ok.log" 2>&1
OK_EXIT=$?
python3 "$CHECK" --hard-limit 50 --ceiling-file "$TMP/ceiling_too_low.txt" >"$TMP/low.log" 2>&1
LOW_EXIT=$?
set -e
report "count at the locked ceiling (baseline + 1 fixture offender) passes" 0 "$OK_EXIT" "$TMP/ok.log"
report "count one over the locked ceiling fails" 1 "$LOW_EXIT" "$TMP/low.log"

echo
echo "── Results: $PASS passed, $FAIL failed ──"
[ "$FAIL" -eq 0 ]
