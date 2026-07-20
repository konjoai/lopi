#!/usr/bin/env bash
# Ensures `lopi sail` (the web dashboard + agent pool) is up, idempotently,
# and prints the URL to open. Closes the one manual step left in the
# Browser-Pane-1 flow ("start sail first") — see CLAUDE.md's "Live
# Dashboard (Browser Pane)" section for what happens after this.
#
# Usage: scripts/start-dashboard.sh [any real `lopi sail` flag]
#   e.g. scripts/start-dashboard.sh --port 3001 --repo ~/myrepo --max-agents 8
# All flags are passed through to `lopi sail` unchanged — this is not a
# second config surface, just a thin idempotent wrapper around it.
#
# What this deliberately does NOT do:
#   - open or navigate the Browser pane itself. There's no reliable way to
#     drive that from outside the app, and it's unnecessary: Claude already
#     finds and opens a reachable `sail` dashboard on its own once it's up.
#   - install a permanent background service (launchd/systemd). This is a
#     per-session convenience script, not infrastructure.
#   - anything OS-specific beyond one optional `open -a Claude` on macOS.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Overridable so tests can stand in a fake binary; real usage never sets this.
LOPI_CMD="${LOPI_CMD:-cargo run --}"
read -ra LOPI_CMD_ARR <<<"$LOPI_CMD"

PORT="3000"
HOST="127.0.0.1"
prev=""
for arg in "$@"; do
  case "$prev" in
  --port | -p) PORT="$arg" ;;
  --host) HOST="$arg" ;;
  esac
  case "$arg" in
  --port=*) PORT="${arg#*=}" ;;
  --host=*) HOST="${arg#*=}" ;;
  esac
  prev="$arg"
done

HEALTH_URL="http://${HOST}:${PORT}/api/health"
LOG_DIR="${HOME}/.lopi"
LOG_FILE="${LOG_DIR}/sail.log"

is_healthy() {
  curl -fsS -o /dev/null --max-time 2 "$HEALTH_URL"
}

if is_healthy; then
  echo "lopi sail is already running at http://${HOST}:${PORT} — nothing to do."
  exit 0
fi

mkdir -p "$LOG_DIR"

nohup "${LOPI_CMD_ARR[@]}" sail "$@" >>"$LOG_FILE" 2>&1 &
disown

echo "starting lopi sail (log: ${LOG_FILE}) ..."
for _ in $(seq 1 60); do
  if is_healthy; then
    echo "lopi sail is up — http://${HOST}:${PORT}"
    if [[ "$(uname -s)" == "Darwin" ]] && ! pgrep -xq "Claude" 2>/dev/null; then
      open -a Claude || true
    fi
    exit 0
  fi
  sleep 1
done

echo "lopi sail did not become healthy within 60s — check ${LOG_FILE}" >&2
exit 1
