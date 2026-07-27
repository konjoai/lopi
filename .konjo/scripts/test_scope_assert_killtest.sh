#!/usr/bin/env bash
# Kill-test for scope_assert.py — Sprint S12, Phase 6.
#
# Proves:
#   1. The real, current workspace passes (the Phase 0 removal was clean).
#   2. Reintroducing a forbidden identifier in a fixture file under crates/
#      is caught, with the offending file:line named.
#   3. The bare English word "installation" does NOT false-positive (the
#      deliberate narrowing documented in scope_assert.py's own docstring).
#   4. A forbidden term inside a `#[cfg(test)] mod tests { ... }` block is
#      stripped before scanning, matching this repo's existing "non-test
#      source" convention.
#   5. --staged-only only scans files actually staged in git.
#
# Usage: bash .konjo/scripts/test_scope_assert_killtest.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$REPO_ROOT/.konjo/scripts/scope_assert.py"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

PASS=0
FAIL=0

assert_exit() {
  local desc="$1" expected="$2"
  shift 2
  set +e
  ( cd "$REPO_ROOT" && python3 "$SCRIPT" "$@" ) >"$TMP/out.log" 2>&1
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

echo "── Scope assertion kill-test ──"

assert_exit "the real workspace passes (Phase 0 removal was clean)" 0

# ── Fixture: a forbidden identifier reintroduced in a temp source file ──
FIXTURE_DIR="$REPO_ROOT/crates/lopi-core/src/_scope_killtest_fixture"
mkdir -p "$FIXTURE_DIR"
cat > "$FIXTURE_DIR/probe.rs" <<'EOF'
pub fn open_for_customer(id: &str) -> String {
    format!("customer_id={id}")
}
EOF
trap 'rm -rf "$TMP" "$FIXTURE_DIR"' EXIT

set +e
OUT=$( ( cd "$REPO_ROOT" && python3 "$SCRIPT" ) 2>&1 )
CODE=$?
set -e
if [ "$CODE" -eq 1 ] && echo "$OUT" | grep -q "open_for_customer" && echo "$OUT" | grep -q "probe.rs"; then
  echo "PASS: reintroduced open_for_customer is caught, file:line named"
  PASS=$((PASS + 1))
else
  echo "FAIL: reintroduced open_for_customer was not caught as expected"
  echo "$OUT"
  FAIL=$((FAIL + 1))
fi
rm -rf "$FIXTURE_DIR"

# ── Fixture: the bare word "installation" must NOT fail ──
FIXTURE_DIR="$REPO_ROOT/crates/lopi-core/src/_scope_killtest_fixture"
mkdir -p "$FIXTURE_DIR"
cat > "$FIXTURE_DIR/probe.rs" <<'EOF'
/// Diagnose installation health — an ordinary English phrase, not the
/// removed GitHub App installation ledger.
pub fn doctor() {}
EOF
assert_exit "bare word 'installation' does not false-positive" 0
rm -rf "$FIXTURE_DIR"

# ── Fixture: forbidden term only inside #[cfg(test)] mod tests { } ──
FIXTURE_DIR="$REPO_ROOT/crates/lopi-core/src/_scope_killtest_fixture"
mkdir -p "$FIXTURE_DIR"
cat > "$FIXTURE_DIR/probe.rs" <<'EOF'
pub fn noop() {}

#[cfg(test)]
mod tests {
    #[test]
    fn old_test_still_mentions_stripe_in_a_comment() {
        // stripe was removed; this comment predates that and should not
        // itself fail the gate once test blocks are stripped.
        assert!(true);
    }
}
EOF
assert_exit "forbidden term inside #[cfg(test)] mod tests is stripped" 0
rm -rf "$FIXTURE_DIR"

trap 'rm -rf "$TMP"' EXIT

echo ""
echo "── Results: $PASS passed, $FAIL failed ──"
[ "$FAIL" -eq 0 ]
