#!/usr/bin/env bash
# Runs each capture state as a fully separate node process invocation
# (node scripts/capture.mjs --state Sn), so every state gets a fresh OS
# process/heap/event loop instead of sharing one long-running node process
# across the whole batch. See the comment above `requestedState` in
# capture.mjs for why: unique-port-per-state fixed the S5 zombie-server
# flake, but a second, different flake (a task-id race at S3) still showed
# up once, pointing at cumulative load inside one long process rather than
# anything port- or server-specific.
set -euo pipefail
cd "$(dirname "$0")/.."

STATES=(S1 S2 S3 S4 S5 S9 S10 S11 S12)

for state in "${STATES[@]}"; do
  echo ""
  echo "##### running $state as its own process #####"
  node scripts/capture.mjs --state "$state"
done

echo ""
echo "##### all states complete #####"
