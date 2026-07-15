#!/usr/bin/env bash
# MAXX kill tests 1-2 — instrumented `lopi run`
#
# Usage (from the repo root, on real hardware with real Claude Code auth):
#   bash .konjo/scripts/quota-kill-test-log.sh --goal "..." --repo /path/to/scratch/clone
#
# What this does:
#   Runs `cargo run -- run <your args>` with LOPI_QUOTA_KILL_TEST_LOG set, so
#   every `rate_limit_event` NDJSON line the CLI emits gets logged — with a
#   timestamp, the raw rate_limit_info payload (including surpassedThreshold/
#   isUsingOverage, which the app's own AgentEvent::ApiRetry doesn't carry),
#   and how many stream events/turns passed since the previous observation.
#
# This does NOT pipe lopi's own stdout: `lopi run`'s process consumes the
# `claude` CLI's NDJSON internally (crates/lopi-agent/src/claude.rs) and never
# forwards it raw to its own stdout, so a `| tee` around this command would
# capture nothing useful. The env var is the actual hook point
# (crates/lopi-agent/src/quota_kill_log.rs) — this script exists so nobody has
# to rediscover that at the keyboard.
#
# Per the sprint brief's pre-flight: run this across low/mid/high utilization
# in one session (or several, same log path) to answer kill test 1 — read the
# resulting log with `jq . <path>` and look at `events_since_last`: a small,
# steady number every time means "fires every turn"; a large first gap that
# then shrinks means threshold-gated. `resets_at`/`surpassed_threshold`/
# `is_using_overage` being null vs present, per `limit_type`, answers kill
# test 1's second question directly.
#
# STANDING GUIDANCE: never point this at the repo you're editing — `--repo`
# should be a throwaway clone. lopi's GitManager checks out `lopi/<taskid>-
# attempt-N` branches in the backend's cwd and `git clean`s untracked files.

set -euo pipefail

LOG_PATH="${LOPI_QUOTA_KILL_TEST_LOG:-quota-kill-test-$(date +%s 2>/dev/null || echo run).jsonl}"

echo "Logging rate_limit_event observations to: ${LOG_PATH}"
echo "(env var LOPI_QUOTA_KILL_TEST_LOG — set it yourself to reuse one log across several runs)"
echo

LOPI_QUOTA_KILL_TEST_LOG="${LOG_PATH}" cargo run -- run "$@"

echo
echo "Done. To read the log:"
echo "  jq . \"${LOG_PATH}\""
echo
echo "To answer kill test 1 at a glance:"
echo "  jq '.events_since_last' \"${LOG_PATH}\"   # steady+small = every turn; one big gap then small = threshold-gated"
echo "  jq '{limit_type, resets_at, surpassed_threshold}' \"${LOG_PATH}\"   # kill test 1's resetsAt-reliability question"
