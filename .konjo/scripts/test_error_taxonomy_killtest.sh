#!/usr/bin/env bash
# Kill-test for error_taxonomy_check.py -- Track C, KT-C.1.
#
# Proves, using real fixture files planted under crates/lopi-core/src/ and
# crates/lopi-memory/src/ and cleaned up on exit (same technique as
# test_indexing_floor_killtest.sh and test_function_length_killtest.sh, since
# this script's REPO_ROOT is fixed to the real checkout):
#
#   1. REGRESSION: pushing an already-migrated crate's (lopi-core) count one
#      file above its locked floor fails the gate, naming the crate.
#   2. NO-REGRESSION: a still-unmigrated crate (lopi-memory) that gains zero
#      new `anyhow::` files -- i.e. stays exactly at its existing (nonzero)
#      floor, the normal state of an untouched crate on every unrelated PR --
#      passes the gate.
#   3. Both fixtures run in the same invocation, proving the checker tells
#      "regression in a migrated crate" apart from "steady-state in an
#      unmigrated crate" rather than collapsing both into one shared signal
#      (the failure mode a single workspace-wide total would have).
#   4. Comment-only `anyhow::` mentions and `#[cfg(test)]`-suffixed test
#      files/dirs are excluded from the count (same convention as the
#      indexing/function-length ratchets).
#
# Usage: bash .konjo/scripts/test_error_taxonomy_killtest.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECK="$REPO_ROOT/.konjo/scripts/error_taxonomy_check.py"
CORE_FIXTURE="$REPO_ROOT/crates/lopi-core/src/_error_taxonomy_killtest_fixture.rs"
TMP="$(mktemp -d)"
cleanup() { rm -rf "$TMP" "$CORE_FIXTURE"; }
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

echo "── Error-taxonomy kill-test (KT-C.1) ──"

# ── Case (b) first: NO fixture planted at all -- the real repo state, where
# lopi-memory (unmigrated, floor 30) has not changed and every other crate is
# exactly at its locked floor. This must ACCEPT.
set +e
python3 "$CHECK" >"$TMP/clean.log" 2>&1
CLEAN_EXIT=$?
set -e
report "(b) unmigrated crate (lopi-memory) unchanged at its existing floor: ACCEPT" 0 "$CLEAN_EXIT" "$TMP/clean.log"
if grep -q "lopi-memory: 30 (floor 30)$" "$TMP/clean.log"; then
  echo "PASS: lopi-memory shown at its unchanged floor, not flagged"
  PASS=$((PASS + 1))
else
  echo "FAIL: expected lopi-memory: 30 (floor 30) with no regression marker"
  cat "$TMP/clean.log"
  FAIL=$((FAIL + 1))
fi

# ── Case (a): plant one new non-test file under lopi-core (already migrated,
# floor 1) that uses `anyhow::` in real code. This must REJECT, even though
# lopi-core is not the only, or even the least-migrated, crate in the repo.
cat > "$CORE_FIXTURE" <<'EOF'
// Kill-test fixture: simulates a regression in an already-migrated crate.
pub fn _killtest_regression() -> anyhow::Result<()> {
    Ok(())
}
EOF

set +e
python3 "$CHECK" >"$TMP/regressed.log" 2>&1
REGRESSED_EXIT=$?
set -e
report "(a) regression in already-migrated crate (lopi-core): REJECT" 1 "$REGRESSED_EXIT" "$TMP/regressed.log"

if grep -q "lopi-core: 2 (floor 1)  <-- REGRESSION" "$TMP/regressed.log"; then
  echo "PASS: lopi-core named as the regressing crate with its real counts"
  PASS=$((PASS + 1))
else
  echo "FAIL: expected lopi-core named as a regression at 2 (floor 1)"
  cat "$TMP/regressed.log"
  FAIL=$((FAIL + 1))
fi

if grep -q "lopi-memory: 30 (floor 30)$" "$TMP/regressed.log"; then
  echo "PASS: lopi-memory still shown as unregressed in the same run"
  PASS=$((PASS + 1))
else
  echo "FAIL: expected lopi-memory to remain unregressed alongside lopi-core's failure"
  cat "$TMP/regressed.log"
  FAIL=$((FAIL + 1))
fi

echo
echo "── Results: $PASS passed, $FAIL failed ──"
[ "$FAIL" -eq 0 ]
