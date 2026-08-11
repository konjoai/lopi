#!/usr/bin/env bash
# Konjo Gate aggregator verdict — Sprint Gate-Tiering-1.
#
# Extracted out of konjo-gate.yml's "Evaluate all required gates" step so the
# tiering + break-glass logic is testable outside Actions (see
# test_gate_tiering_killtest.sh), matching the pattern every other
# .konjo/scripts check already follows.
#
# Reads job results from environment variables (set by the workflow step from
# needs.<job>.result) and decides the aggregator's exit code.
#
# BLOCKING tier (must be "success", no skip exemption): STATIC, COVERAGE.
# ADVISORY tier (reported, never blocks): DOC_STALENESS, WEB_AUDIT, COMPLEXITY,
#   MUTATION, REVIEW, KONJO_GATES. MUTATION and REVIEW additionally accept
#   "skipped" as non-failure — they are PR-only jobs that legitimately skip on
#   a push-to-main run (unchanged from the pre-tiering aggregator).
#
# Break-glass: a `gate:override` PR label (GATE_OVERRIDE_LABEL=true) requires
# a `Konjo-Override: <reason>` trailer in PR_BODY. Missing trailer fails
# outright, regardless of gate results — the escape hatch must cost a
# sentence, not nothing. With a valid trailer, a BLOCKING-tier failure is
# reported OVERRIDDEN and exits 0.
#
# Exit codes:
#   0 — every BLOCKING-tier job succeeded (or a valid override applies)
#   1 — a BLOCKING-tier job failed with no valid override, or the
#       gate:override label is present without its required trailer

set -euo pipefail

DOC_STALENESS="${DOC_STALENESS:-}"
STATIC="${STATIC:-}"
WEB_AUDIT="${WEB_AUDIT:-}"
COVERAGE="${COVERAGE:-}"
COMPLEXITY="${COMPLEXITY:-}"
MUTATION="${MUTATION:-}"
REVIEW="${REVIEW:-}"
KONJO_GATES="${KONJO_GATES:-}"
GATE_OVERRIDE_LABEL="${GATE_OVERRIDE_LABEL:-false}"
PR_BODY="${PR_BODY:-}"
ACTOR="${ACTOR:-unknown}"

echo "doc-staleness: $DOC_STALENESS"
echo "static:      $STATIC"
echo "web-audit:   $WEB_AUDIT"
echo "coverage:    $COVERAGE"
echo "complexity:  $COMPLEXITY"
echo "mutation:    $MUTATION"
echo "review:      $REVIEW"
echo "konjo-gates: $KONJO_GATES"

# ── gate:override requires its trailer up front, independent of outcome ──
if [ "$GATE_OVERRIDE_LABEL" = "true" ]; then
  if ! printf '%s' "$PR_BODY" | grep -qE '^Konjo-Override:[[:space:]]*[^[:space:]].*$'; then
    echo "::error::'gate:override' label present but the PR body has no 'Konjo-Override: <reason>' trailer. The escape hatch must cost a sentence, not nothing."
    exit 1
  fi
fi

# ── BLOCKING tier: must succeed, no skip exemption ──
FAILED_BLOCKING=""
[ "$STATIC" != "success" ] && FAILED_BLOCKING="$FAILED_BLOCKING static"
[ "$COVERAGE" != "success" ] && FAILED_BLOCKING="$FAILED_BLOCKING coverage"

# ── ADVISORY tier: reported, never blocks. mutation/review accept "skipped" ──
FAILED_ADVISORY=""
[ "$DOC_STALENESS" != "success" ] && FAILED_ADVISORY="$FAILED_ADVISORY doc-staleness"
[ "$WEB_AUDIT" != "success" ] && FAILED_ADVISORY="$FAILED_ADVISORY web-audit"
[ "$COMPLEXITY" != "success" ] && FAILED_ADVISORY="$FAILED_ADVISORY complexity"
[ "$MUTATION" != "success" ] && [ "$MUTATION" != "skipped" ] && FAILED_ADVISORY="$FAILED_ADVISORY mutation"
[ "$REVIEW" != "success" ] && [ "$REVIEW" != "skipped" ] && FAILED_ADVISORY="$FAILED_ADVISORY review"
[ "$KONJO_GATES" != "success" ] && FAILED_ADVISORY="$FAILED_ADVISORY konjo-gates"

if [ -n "$FAILED_BLOCKING" ]; then
  if [ "$GATE_OVERRIDE_LABEL" = "true" ]; then
    echo ""
    echo "OVERRIDDEN — gate:override label present with a verified Konjo-Override trailer."
    echo "label: gate:override"
    echo "actor: $ACTOR"
    echo "BLOCKING gate(s) bypassed:$FAILED_BLOCKING"
    echo "::warning::Merge proceeded under gate:override. Record a LEDGER.md entry for this override."
    if [ -n "$FAILED_ADVISORY" ]; then
      echo ""
      echo "ADVISORY (non-blocking) failures:$FAILED_ADVISORY"
    fi
    exit 0
  fi

  echo "::error::BLOCKING gate(s) failed:$FAILED_BLOCKING — merge blocked."
  if [ -n "$FAILED_ADVISORY" ]; then
    echo ""
    echo "ADVISORY (non-blocking) failures:$FAILED_ADVISORY"
  fi
  exit 1
fi

if [ -n "$FAILED_ADVISORY" ]; then
  echo ""
  echo "ADVISORY (non-blocking) failures:$FAILED_ADVISORY"
fi

echo ""
echo "All BLOCKING Konjo quality gates passed ✓"
echo "The code is seaworthy."
