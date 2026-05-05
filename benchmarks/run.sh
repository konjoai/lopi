#!/usr/bin/env bash
# lopi benchmark runner — executes the T01–T10 corpus sequentially and reports metrics.
# Usage: ./benchmarks/run.sh [--repo <path>] [--tasks T01,T02,...] [--dry-run]
set -euo pipefail

REPO="${LOPI_BENCH_REPO:-$(git rev-parse --show-toplevel)}"
DRY_RUN=false
TASK_FILTER=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --repo) REPO="$2"; shift 2 ;;
        --tasks) TASK_FILTER="$2"; shift 2 ;;
        --dry-run) DRY_RUN=true; shift ;;
        *) echo "Unknown arg: $1"; exit 1 ;;
    esac
done

TIMESTAMP=$(date -u +"%Y%m%dT%H%M%SZ")
OUTDIR="benchmarks/results/${TIMESTAMP}_corpus"
mkdir -p "$OUTDIR"

declare -A TASK_GOALS
TASK_GOALS[T01]="Add a unit test for the jaccard_similarity function in lopi-memory"
TASK_GOALS[T02]="Add PartialEq derive to AgentState in lopi-core and fix all match exhaustiveness"
TASK_GOALS[T03]="Implement Display for TaskStatus in lopi-core that produces human-readable output"
TASK_GOALS[T04]="Add created_at index to the patterns table in lopi-memory schema.sql"
TASK_GOALS[T05]="Add a --verbose flag to lopi run that prints raw claude output to stdout"
TASK_GOALS[T06]="Refactor runner.rs to extract the plan+implement+fix attempt loop into a named method"
TASK_GOALS[T07]="Add GET /api/metrics endpoint to lopi-ui web dashboard returning PoolStats as JSON"
TASK_GOALS[T08]="Implement retry_with_backoff in lopi-agent runner.rs for transient IO errors"
TASK_GOALS[T09]="Add lopi bench CLI subcommand that runs T01-T10 corpus tasks sequentially"
TASK_GOALS[T10]="Integrate AnthropicLimiter from lopi-ratelimit into AgentPool for TPM and RPM enforcement"

PASS=0
FAIL=0
SKIP=0

echo "lopi benchmark corpus — $TIMESTAMP"
echo "repo: $REPO"
echo "output: $OUTDIR"
echo ""
printf "%-6s  %-60s  %-8s  %s\n" "Task" "Goal" "Status" "Wall time"
printf "%s\n" "$(printf '─%.0s' {1..100})"

for TASK_ID in T01 T02 T03 T04 T05 T06 T07 T08 T09 T10; do
    if [[ -n "$TASK_FILTER" ]] && [[ "$TASK_FILTER" != *"$TASK_ID"* ]]; then
        printf "%-6s  %-60s  %-8s\n" "$TASK_ID" "(skipped by filter)" "SKIP"
        SKIP=$((SKIP + 1))
        continue
    fi

    GOAL="${TASK_GOALS[$TASK_ID]}"
    GOAL_SHORT="${GOAL:0:58}"
    TASK_LOG="$OUTDIR/${TASK_ID}.log"
    START_TS=$(date +%s)

    if $DRY_RUN; then
        printf "%-6s  %-60s  %-8s  %s\n" "$TASK_ID" "$GOAL_SHORT" "DRY-RUN" "0s"
        continue
    fi

    if lopi run --goal "$GOAL" --repo "$REPO" >"$TASK_LOG" 2>&1; then
        STATUS="PASS"
        PASS=$((PASS + 1))
    else
        STATUS="FAIL"
        FAIL=$((FAIL + 1))
    fi

    END_TS=$(date +%s)
    ELAPSED=$((END_TS - START_TS))
    printf "%-6s  %-60s  %-8s  %ds\n" "$TASK_ID" "$GOAL_SHORT" "$STATUS" "$ELAPSED"
done

echo ""
echo "Results saved to: $OUTDIR"
echo "Pass: $PASS  Fail: $FAIL  Skip: $SKIP  Total: $((PASS + FAIL + SKIP))"

# Write machine-readable summary.
cat > "$OUTDIR/summary.json" <<EOF
{
  "timestamp": "$TIMESTAMP",
  "repo": "$REPO",
  "pass": $PASS,
  "fail": $FAIL,
  "skip": $SKIP,
  "total": $((PASS + FAIL + SKIP))
}
EOF
