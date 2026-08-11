#!/usr/bin/env bash
# Kill-test for gate_verdict.sh — Sprint Gate-Tiering-1.
#
# Proves the four properties the tiering/break-glass rewrite of
# konjo-gate.yml's aggregator depends on, by constructing synthetic
# `needs`-result inputs (env vars) and asserting gate_verdict.sh's exit code.
# See gate_verdict.sh's own header for the tier assignment and override rule
# this test is pinned against.
#
# Usage: bash .konjo/scripts/test_gate_tiering_killtest.sh

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$REPO_ROOT/.konjo/scripts/gate_verdict.sh"

PASS=0
FAIL=0
LAST_OUT=""
LAST_CODE=0

# All-green baseline env, overridable per-call via the trailing "VAR=val" args.
run_verdict() {
  LAST_OUT=$(env -i PATH="$PATH" \
    DOC_STALENESS=success \
    STATIC=success \
    WEB_AUDIT=success \
    COVERAGE=success \
    COMPLEXITY=success \
    MUTATION=skipped \
    REVIEW=skipped \
    KONJO_GATES=success \
    GATE_OVERRIDE_LABEL=false \
    PR_BODY="" \
    ACTOR=test-actor \
    "$@" \
    bash "$SCRIPT" 2>&1)
  LAST_CODE=$?
}

check_exit() {
  local desc="$1" expected="$2"
  if [ "$LAST_CODE" -eq "$expected" ]; then
    echo "PASS: $desc (exit $LAST_CODE)"
    PASS=$((PASS + 1))
  else
    echo "FAIL: $desc (expected exit $expected, got $LAST_CODE)"
    echo "$LAST_OUT"
    FAIL=$((FAIL + 1))
  fi
}

check_contains() {
  local desc="$1" needle="$2"
  if echo "$LAST_OUT" | grep -qF "$needle"; then
    echo "PASS: $desc"
    PASS=$((PASS + 1))
  else
    echo "FAIL: $desc — output did not contain: $needle"
    echo "$LAST_OUT"
    FAIL=$((FAIL + 1))
  fi
}

echo "── Gate-tiering kill-test ──"

# ── 1. A BLOCKING-tier failure exits non-zero ──
run_verdict STATIC=failure
check_exit "BLOCKING-tier failure (static) exits non-zero" 1
check_contains "static named as the failing BLOCKING gate" "BLOCKING gate(s) failed: static"

# ── 2. An ADVISORY-tier failure exits zero and is named in the advisory report ──
run_verdict WEB_AUDIT=failure
check_exit "ADVISORY-tier failure (web-audit) exits zero" 0
check_contains "web-audit named in the ADVISORY report" "ADVISORY (non-blocking) failures: web-audit"

# ── 3. skipped on mutation/review exits zero ──
run_verdict MUTATION=skipped REVIEW=skipped
check_exit "skipped mutation + review (PR-only jobs) exits zero" 0

# also prove a genuine mutation/review FAILURE (not skipped) is still
# reported as an advisory failure, not silently accepted
run_verdict MUTATION=failure REVIEW=failure
check_exit "failed (not skipped) mutation/review exits zero but is reported" 0
check_contains "mutation named in the ADVISORY report" "mutation"
check_contains "review named in the ADVISORY report" "review"

# ── 4. gate:override with a Konjo-Override: trailer exits zero on a BLOCKING
#        failure; without the trailer it exits non-zero ──
run_verdict STATIC=failure GATE_OVERRIDE_LABEL=true \
  PR_BODY=$'Some PR description.\n\nKonjo-Override: known flaky runner, re-run confirmed clean locally.'
check_exit "gate:override + trailer exits zero on BLOCKING failure" 0
check_contains "OVERRIDDEN reported with trailer present" "OVERRIDDEN"

run_verdict STATIC=failure GATE_OVERRIDE_LABEL=true \
  PR_BODY="Some PR description with no trailer at all."
check_exit "gate:override WITHOUT trailer exits non-zero" 1

run_verdict GATE_OVERRIDE_LABEL=true PR_BODY="No trailer here either."
check_exit "gate:override label with no failures still requires trailer" 1

echo ""
echo "── Results: $PASS passed, $FAIL failed ──"
[ "$FAIL" -eq 0 ]
