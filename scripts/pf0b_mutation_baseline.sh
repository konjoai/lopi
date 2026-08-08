#!/usr/bin/env bash
# PF-0b: per-crate incremental mutation baseline, smallest-first.
# See NEXT_SESSION_PROMPT.md / LEDGER.md Review-Pipeline-Phase-2 for why: the
# 5,315-mutant full-workspace run died twice on this class of session
# environment (idle-between-turns suspension). Per-crate runs are resumable --
# a dead session loses one crate's progress, not everything.
set -u
cd "$(dirname "$0")/.."
OUT_ROOT="bench_results/lopi"
SUMMARY="$OUT_ROOT/pf0b_summary.jsonl"
LOG="$OUT_ROOT/pf0b_progress.log"
mkdir -p "$OUT_ROOT"

# name:timeout_seconds, smallest crate (by .rs LOC) first.
CRATES=(
  "lopi-github:600"
  "lopi-remote:600"
  "lopi-tools:600"
  "lopi-ratelimit:600"
  "lopi-spec:900"
  "lopi-webhook:900"
  "lopi-demo:900"
  "lopi-skill:1200"
  "lopi-context:1200"
  "lopi-mcp:1200"
  "lopi-toon:1200"
  "lopi-git:1200"
  "lopi-index:1800"
  "lopi-orchestrator:1800"
  "lopi-memory:1800"
  "lopi-core:1800"
  "lopi-ui:1800"
  "lopi-agent:1800"
)

echo "PF-0b run started $(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$LOG"

for entry in "${CRATES[@]}"; do
  crate="${entry%%:*}"
  budget="${entry##*:}"
  ts="$(date -u +%Y%m%dT%H%M%SZ)"
  out_dir="$OUT_ROOT/${crate}_${ts}"
  start_epoch=$(date +%s)
  echo "=== $crate (budget ${budget}s) start $(date -u +%Y-%m-%dT%H:%M:%SZ) ===" >> "$LOG"
  timeout "$budget" cargo mutants -p "$crate" --output "$out_dir" --jobs 2 >> "$LOG" 2>&1
  rc=$?
  end_epoch=$(date +%s)
  elapsed=$((end_epoch - start_epoch))
  status="complete"
  if [ "$rc" -eq 124 ]; then
    status="timeout_partial"
  elif [ "$rc" -ne 0 ]; then
    status="error_rc_${rc}"
  fi
  outcomes="$out_dir/mutants.out/outcomes.json"
  caught="null"; missed="null"; unviable="null"; timeout_n="null"; total="null"
  if [ -f "$outcomes" ]; then
    read -r caught missed unviable timeout_n total <<EOF_PY
$(python3 - "$outcomes" <<'PYEOF'
import json, sys
try:
    data = json.load(open(sys.argv[1]))
except Exception:
    print("null null null null null")
    raise SystemExit
outcomes = data.get("outcomes", [])
c = sum(1 for o in outcomes if o.get("summary") == "CaughtMutant")
m = sum(1 for o in outcomes if o.get("summary") == "MissedMutant")
u = sum(1 for o in outcomes if o.get("summary") == "Unviable")
t = sum(1 for o in outcomes if o.get("summary") == "Timeout")
print(c, m, u, t, len(outcomes))
PYEOF
)
EOF_PY
  fi
  python3 - "$SUMMARY" "$crate" "$status" "$elapsed" "$budget" "$out_dir" "$caught" "$missed" "$unviable" "$timeout_n" "$total" <<'PYEOF'
import json, sys
summary, crate, status, elapsed, budget, out_dir, caught, missed, unviable, timeout_n, total = sys.argv[1:12]
def num(x):
    return None if x == "null" else int(x)
rec = {
    "crate": crate, "status": status, "elapsed_s": int(elapsed), "budget_s": int(budget),
    "out_dir": out_dir, "caught": num(caught), "missed": num(missed),
    "unviable": num(unviable), "timeout": num(timeout_n), "total_tested": num(total),
}
with open(summary, "a") as f:
    f.write(json.dumps(rec) + "\n")
print(f"  -> {rec}")
PYEOF
  echo "=== $crate done: status=$status elapsed=${elapsed}s ===" >> "$LOG"
done

echo "PF-0b run finished $(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$LOG"
