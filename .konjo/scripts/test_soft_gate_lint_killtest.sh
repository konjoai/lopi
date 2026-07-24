#!/usr/bin/env bash
# Kill-test for soft_gate_lint.py — Sprint S4 Phase 2 verification.
#
# Proves:
#   1. The lint PASSES against the real, current .github/workflows/konjo-gate.yml
#      (every continue-on-error: true step is already declared).
#   2. The lint FAILS when a bare continue-on-error: true (no declaring
#      comment) is injected into a fixture copy, pointing at the offending
#      line. A pass here would mean the lint is broken, not that a
#      hypothetical file is clean.
#   3. Declared forms — both `KNOWN DEBT, verified <date>` + a next step,
#      and `ADVISORY BY DESIGN` — are each individually accepted.
#
# Usage: bash .konjo/scripts/test_soft_gate_lint_killtest.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LINT="$REPO_ROOT/.konjo/scripts/soft_gate_lint.py"
WORKFLOW="$REPO_ROOT/.github/workflows/konjo-gate.yml"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

PASS=0
FAIL=0

assert_exit() {
  local desc="$1" expected="$2"
  shift 2
  set +e
  python3 "$LINT" "$@" >"$TMP/out.log" 2>&1
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

echo "── Soft-gate lint kill-test ──"

assert_exit "current konjo-gate.yml passes (every soft gate declared)" 0 "$WORKFLOW"

# ── Fixture: bare continue-on-error: true, no declaring comment ──
cat > "$TMP/bare_gate.yml" <<'EOF'
jobs:
  test:
    steps:
      - name: Some step with no explanation
        continue-on-error: true
        run: echo hi
EOF
assert_exit "bare continue-on-error: true (no comment) fails" 1 "$TMP/bare_gate.yml"

# ── Fixture: KNOWN DEBT + next step, correctly declared ──
cat > "$TMP/known_debt.yml" <<'EOF'
jobs:
  test:
    steps:
      - name: A dated, honest soft gate
        # KNOWN DEBT, verified 2026-07-24: something is broken right now.
        # Next step: fix the thing, then remove this line.
        continue-on-error: true
        run: echo hi
EOF
assert_exit "KNOWN DEBT + next step passes" 0 "$TMP/known_debt.yml"

# ── Fixture: ADVISORY BY DESIGN, correctly declared ──
cat > "$TMP/advisory.yml" <<'EOF'
jobs:
  test:
    steps:
      - name: A permanently-soft advisory gate
        # ADVISORY BY DESIGN: this is opinionated, not correctness-bearing.
        continue-on-error: true
        run: echo hi
EOF
assert_exit "ADVISORY BY DESIGN passes" 0 "$TMP/advisory.yml"

# ── Fixture: KNOWN DEBT with a verified date but no next step ──
cat > "$TMP/no_next_step.yml" <<'EOF'
jobs:
  test:
    steps:
      - name: Dated but no plan
        # KNOWN DEBT, verified 2026-07-24: something is broken right now.
        continue-on-error: true
        run: echo hi
EOF
assert_exit "KNOWN DEBT without a next step still fails" 1 "$TMP/no_next_step.yml"

echo
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
