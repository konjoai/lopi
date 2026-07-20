#!/usr/bin/env bash
# Exercises scripts/start-dashboard.sh's health-check-first logic for real —
# no mocking framework, just a fake `lopi` stub (LOPI_CMD) swapped in so the
# test never needs a real cargo build. Covers the three success criteria from
# the Startup-Script-1 sprint: idempotent double-run, already-running
# detection, and correctly detecting the process is gone after it's killed.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

WORKDIR="$(mktemp -d)"
PIDS_TO_KILL=()
cleanup() {
  for pid in "${PIDS_TO_KILL[@]:-}"; do
    kill "$pid" >/dev/null 2>&1 || true
  done
  rm -rf "$WORKDIR"
}
trap cleanup EXIT

PASS=0
FAIL=0
ok() {
  echo "  ok - $1"
  PASS=$((PASS + 1))
}
bad() {
  echo "  FAIL - $1"
  FAIL=$((FAIL + 1))
}

free_port() {
  python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])'
}

# Minimal /api/health responder standing in for `lopi sail`.
cat >"$WORKDIR/fake_health_server.py" <<'PY'
import http.server
import sys

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/api/health":
            body = b'{"status":"ok","service":"lopi"}'
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        else:
            self.send_response(404)
            self.end_headers()

    def log_message(self, *_args):
        pass

port = int(sys.argv[1])
http.server.HTTPServer(("127.0.0.1", port), Handler).serve_forever()
PY

# Fake "lopi" binary: real `start-dashboard.sh` invokes `$LOPI_CMD sail <flags...>`
cat >"$WORKDIR/fake-lopi-ok" <<EOF
#!/usr/bin/env bash
port=3000
prev=""
for arg in "\$@"; do
  case "\$prev" in --port|-p) port="\$arg" ;; esac
  prev="\$arg"
done
exec python3 "$WORKDIR/fake_health_server.py" "\$port"
EOF
chmod +x "$WORKDIR/fake-lopi-ok"

# Fake "lopi" that must NEVER be invoked — writes a marker if it is, so tests
# can assert the "already running" path really did nothing.
cat >"$WORKDIR/fake-lopi-must-not-run" <<EOF
#!/usr/bin/env bash
touch "$WORKDIR/started-marker"
exit 1
EOF
chmod +x "$WORKDIR/fake-lopi-must-not-run"

echo "== Test 1: already-running is detected and nothing new is started =="
PORT1="$(free_port)"
python3 "$WORKDIR/fake_health_server.py" "$PORT1" &
server_pid=$!
PIDS_TO_KILL+=("$server_pid")
for _ in $(seq 1 20); do
  curl -fsS -o /dev/null --max-time 1 "http://127.0.0.1:${PORT1}/api/health" && break
  sleep 0.2
done

out="$(LOPI_CMD="$WORKDIR/fake-lopi-must-not-run" ./scripts/start-dashboard.sh --port "$PORT1")"
if echo "$out" | grep -qi "already running"; then
  ok "prints an already-running message"
else
  bad "expected an already-running message, got: $out"
fi
if [[ -e "$WORKDIR/started-marker" ]]; then
  bad "start command was invoked even though health check passed"
else
  ok "did not invoke the start command"
fi
kill "$server_pid" >/dev/null 2>&1 || true

echo "== Test 2: not running -> starts fresh and becomes healthy =="
PORT2="$(free_port)"
out="$(LOPI_CMD="$WORKDIR/fake-lopi-ok" ./scripts/start-dashboard.sh --port "$PORT2")"
if echo "$out" | grep -qi "lopi sail is up"; then
  ok "reports success once healthy"
else
  bad "expected a success message, got: $out"
fi
if curl -fsS -o /dev/null --max-time 2 "http://127.0.0.1:${PORT2}/api/health"; then
  ok "health endpoint is actually reachable after start"
else
  bad "health endpoint did not come up"
fi
started_pid="$(pgrep -f "fake_health_server.py $PORT2" | head -1 || true)"
[[ -n "$started_pid" ]] && PIDS_TO_KILL+=("$started_pid")

echo "== Test 3: rerunning while healthy is still a no-op (idempotent double-run) =="
out="$(LOPI_CMD="$WORKDIR/fake-lopi-must-not-run" ./scripts/start-dashboard.sh --port "$PORT2")"
if echo "$out" | grep -qi "already running" && [[ ! -e "$WORKDIR/started-marker" ]]; then
  ok "second run detects the first and starts nothing"
else
  bad "second run did not behave idempotently: $out"
fi

echo "== Test 4: killing the backgrounded process is correctly detected, then restarted =="
if [[ -n "$started_pid" ]]; then
  kill "$started_pid" >/dev/null 2>&1 || true
  for _ in $(seq 1 20); do
    curl -fsS -o /dev/null --max-time 1 "http://127.0.0.1:${PORT2}/api/health" || break
    sleep 0.2
  done
  if curl -fsS -o /dev/null --max-time 1 "http://127.0.0.1:${PORT2}/api/health"; then
    bad "port ${PORT2} still healthy after kill — test setup is wrong"
  else
    ok "health check correctly reports the process is gone"
  fi
  out="$(LOPI_CMD="$WORKDIR/fake-lopi-ok" ./scripts/start-dashboard.sh --port "$PORT2")"
  if echo "$out" | grep -qi "lopi sail is up"; then
    ok "correctly starts a fresh instance after the old one died"
  else
    bad "did not restart after the process was killed: $out"
  fi
  restarted_pid="$(pgrep -f "fake_health_server.py $PORT2" | head -1 || true)"
  [[ -n "$restarted_pid" ]] && PIDS_TO_KILL+=("$restarted_pid")
else
  bad "no pid captured for the process started in Test 2 — cannot run Test 4"
fi

echo
echo "${PASS} passed, ${FAIL} failed"
[[ "$FAIL" -eq 0 ]]
