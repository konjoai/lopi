# Running the lopi multipane Forge

How to start `lopi sail`, launch concurrent live sessions, and open the
multiagent panes in both the web Forge and the macOS app. Every value on a pane
traces to a real `claude -p --output-format stream-json` event — there is no
mock data unless you explicitly pass `?demo=1` (web only).

## Prerequisites

- `claude` CLI authenticated against your subscription (interactive login), with
  no `ANTHROPIC_API_KEY` set. lopi scrubs Anthropic routing env from every spawn,
  so the CLI uses your `~/.claude` credentials.
- `cargo build` green.
- Each session bills against the subscription's included Agent SDK credit, then
  meters. Watch the cost: the Forge surfaces cumulative `total_cost_usd` per
  session (the `cost` event) and a rate-limit pill (the `api_retry` event).

## 1. Start the server

```bash
cargo run -- sail --port 3000 --max-agents 4 --repo .
```

This serves the web Forge at http://127.0.0.1:3000 and exposes the REST + WS API
the macOS app also consumes.

## 2. Launch four concurrent live sessions

Create four scratch repos so the agents work in isolation, then submit one task
per repo to the running server:

```bash
for n in 1 2 3 4; do
  d=$(mktemp -d "/tmp/lopi-session-$n-XXXX")
  ( cd "$d" && git init -q && printf 'fn main(){println!("hi");}\n' > main.rs \
      && git add -A && git commit -qm init )
  curl -s -X POST http://127.0.0.1:3000/api/tasks \
    -H 'content-type: application/json' \
    -d "{\"goal\":\"List the files and summarize main.rs\",\"repo\":\"$d\",\"priority\":\"normal\"}"
  echo " -> queued session $n in $d"
done
```

Each task spawns a real `claude -p` stream. The pool runs up to `--max-agents`
concurrently; the rest queue.

## 3. Open the web Forge

Open http://127.0.0.1:3000 . You should see:

- One tile per live session in the grid (no `demo-*` ids anywhere).
- Per pane: the `ThoughtStream` (assistant text + 🔧 tool calls interleaved),
  the `TokenGauge` (real `token_delta`), `CostAnalytics` (real `cost`),
  `PhaseWheel` / tile status (real `phase`), and `LogStream`.
- Drag a session from the sidebar into a pane, or use +/- to split the layout.
  The layout persists across reloads (`stores/layout.ts`).
- Stop the server and the grid shows an honest "backend offline" state, not
  fabricated agents.

To preview the UI without a backend (explicit opt-in only):
http://127.0.0.1:3000/?demo=1

## 4. Open the macOS app against the same server

```bash
cd macos
xcodegen generate
xcodebuild -project Lopi.xcodeproj -scheme Lopi -destination 'platform=macOS' build
open ~/Library/Developer/Xcode/DerivedData/Lopi-*/Build/Products/Debug/Lopi.app
```

The app connects to `lopi sail` over the same REST + WS API. The Forge tab shows
the same four sessions live in `PaneGridView`; panes update from real events and
the connection LED plus an over-grid banner report offline/empty state honestly
when the server stops.

## Cost ceilings

Per-session `--max-turns` / `--max-budget-usd` caps are a follow-up (gate G7) —
see `artifacts/E2E_REPORT.md` for current status. Until then, keep `--max-agents`
modest and watch the cumulative cost in `CostAnalytics`.
