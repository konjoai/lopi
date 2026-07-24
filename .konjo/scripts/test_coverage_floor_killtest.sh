#!/usr/bin/env bash
# Kill-test for coverage_floor_check.py — Sprint S4 Phase 1 verification.
#
# Proves three things against synthetic lcov.info fixtures (never the real
# one — this must never depend on the current workspace's actual coverage):
#   1. A PR that drops coverage below the stored floor fails the check.
#   2. A PR that holds or raises coverage passes.
#   3. A PR that raises coverage *and* bumps the floor in the same commit
#      passes, and the new floor is what gets compared against.
#
# Also exercises the exact under-scoping bug the gate's own comment warns
# about: this script only ever sums LF:/LH:, so a synthetic multi-file
# lcov.info (mimicking a real --workspace scan) must measure correctly.
#
# Usage: bash .konjo/scripts/test_coverage_floor_killtest.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECK="$SCRIPT_DIR/coverage_floor_check.py"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

PASS=0
FAIL=0

assert_exit() {
  local desc="$1" expected="$2" lcov="$3" floor="$4"
  set +e
  python3 "$CHECK" --lcov "$lcov" --floor-file "$floor" >"$TMP/out.log" 2>&1
  local actual=$?
  set -e
  if [ "$actual" -eq "$expected" ]; then
    echo "PASS: $desc (exit $actual)"
    PASS=$((PASS + 1))
  else
    echo "FAIL: $desc (expected exit $expected, got $actual)"
    cat "$TMP/out.log"
    FAIL=$((FAIL + 1))
  fi
}

# ── Fixture: multi-file lcov mimicking a real --workspace scan, summing to
#    exactly 68.34% (23355 found / 15960 hit — the sprint's real baseline) ──
cat > "$TMP/lcov_at_floor.info" <<'EOF'
SF:crates/lopi-core/src/lib.rs
LF:11678
LH:7980
end_of_record
SF:crates/lopi-agent/src/lib.rs
LF:11677
LH:7980
end_of_record
EOF

cat > "$TMP/floor_68_34.txt" <<'EOF'
# test fixture
68.34
EOF

# ── Fixture: coverage regressed to 60% ──
cat > "$TMP/lcov_regressed.info" <<'EOF'
SF:crates/lopi-core/src/lib.rs
LF:10000
LH:6000
end_of_record
EOF

# ── Fixture: coverage raised to 75%, floor bumped to match in the same "PR" ──
cat > "$TMP/lcov_raised.info" <<'EOF'
SF:crates/lopi-core/src/lib.rs
LF:10000
LH:7500
end_of_record
EOF

cat > "$TMP/floor_bumped_75.txt" <<'EOF'
75.0
EOF

# ── Fixture: missing lcov.info (broken measurement path) ──
NONEXISTENT="$TMP/does_not_exist.info"

echo "── Coverage floor kill-test ──"
assert_exit "holds exactly at floor (68.34% == 68.34%)"          0 "$TMP/lcov_at_floor.info"  "$TMP/floor_68_34.txt"
assert_exit "regression below floor (60% < 68.34%) fails"        1 "$TMP/lcov_regressed.info" "$TMP/floor_68_34.txt"
assert_exit "raise + floor bump in same PR (75% >= 75%) passes"  0 "$TMP/lcov_raised.info"    "$TMP/floor_bumped_75.txt"
assert_exit "raise without bumping floor still passes (>= old)"  0 "$TMP/lcov_raised.info"    "$TMP/floor_68_34.txt"
assert_exit "missing lcov.info measures 0% -> fails, never silently passes" 1 "$NONEXISTENT" "$TMP/floor_68_34.txt"

echo
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
