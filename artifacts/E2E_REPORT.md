# lopi E2E + Multipane — Kill-test & Gate Report

Branch: `feat/real-event-spine-multipane`. Machine: darwin 25.5.0 (this host).
All numbers below were measured, not assumed.

## Pre-flight kill-tests

| ID | Result | Evidence |
|----|--------|----------|
| K0 Toolchain | PASS | `claude` 2.1.153, `cargo` 1.89.0 (Homebrew), `node` v20.19.4, `xcodegen` 2.45.4, `xcodebuild` Xcode 26.6 (17F113). No `ANTHROPIC_API_KEY` in shell. `ANTHROPIC_BASE_URL=https://api.anthropic.com` is set (default public endpoint, not a proxy); the env-scrub strips it from child spawns regardless. `~/.claude.json` shows `oauthAccount: yes` → subscription auth, not API key. |
| K1 Baseline build | PASS | `cargo build` exit 0, finished in 18.33s, no warnings in tail. |
| K2 Baseline tests | PASS | `cargo nextest run` exit 0. `cargo clippy --workspace --all-targets -- -D warnings` exit 0 (clean). |
| K3 Web baseline | PASS (after 1-line fix) | `npm ci` ok. `npm run check` (svelte-check): 0 errors, 2 warnings. `npm test` initially FAILED (exit 1): `package.json` test script referenced `agents-reducer.test.ts` but the file was renamed to `agentReducer.test.ts`. Fixed the script path. All 9 tsx test files now pass: parser 63, agentReducer 28, layout-core 32, session-groups 16, events 24, api 18, badges 14, connections 18, excitement 23. |
| K4 macOS baseline | PASS | `xcodegen generate` ok. `xcodebuild -project Lopi.xcodeproj -scheme Lopi -destination 'platform=macOS' build` exit 0. macOS seeds agents from the real snapshot (no fabricated demo ids). |
| K5 Capture real stream | PASS | 44 NDJSON lines captured to `artifacts/STREAM_CAPTURE.jsonl` from a scratch git repo. Shape documented below. Cost of this single capture: `total_cost_usd = 0.0479`. |
| K6 Reproduce the lie | PASS (code-level) | Web mock generator `initMock()` fabricates `demo-1`..`demo-5` in `web/src/lib/stores/wsClient.ts`; triggered by a 1.5s `setTimeout` fallback at `web/src/lib/stores/agents.ts:297` when state is `offline`/`connecting`. `connectionState` writable already carries a `'mock'` value. macOS `AppModel.swift:243` seeds from the real snapshot via `seedAgent` — no fabricated ids, but seeded state can mask a dead WebSocket. Live browser confirmation of zero `demo-*` is deferred to gate G4. |

## Observed stream shape (the contract the parser is built against)

Captured with:
```
claude -p "List the files in this directory and read one of them" \
  --output-format stream-json --verbose --include-partial-messages \
  --model claude-sonnet-4-6 --max-turns 4
```

Top-level `type` histogram (44 lines):

| count | type | subtype | key fields |
|-------|------|---------|-----------|
| 31 | `stream_event` | — | `event`, `ttft_ms`, `parent_tool_use_id`, `session_id`, `uuid` |
| 4 | `assistant` | — | `message`, `request_id`, `parent_tool_use_id`, `session_id` |
| 3 | `system` | `status` | `status` (`"requesting"`) |
| 2 | `user` | — | `message`, `tool_use_result`, `timestamp` |
| 1 | `system` | `init` | `session_id`, `model`, `cwd`, `tools`, `skills`, `mcp_servers`, `permissionMode`, `claude_code_version`, … |
| 1 | `rate_limit_event` | — | `rate_limit_info` |
| 1 | `system` | `post_turn_summary` | `needs_action`, `status_category` (`"review_ready"`), `status_detail` |
| 1 | `result` | `success` | `total_cost_usd`, `session_id`, `num_turns`, `duration_ms`, `stop_reason`, `terminal_reason`, `usage`, `modelUsage`, `is_error` |

Nested `stream_event.event.type` histogram (31): `content_block_delta` ×14, `content_block_start` ×4, `content_block_stop` ×4, `message_start` ×3, `message_delta` ×3, `message_stop` ×3.

- `content_block_start.content_block.type` ∈ {`text`, `thinking`, `tool_use`}
- `content_block_delta.delta.type` ∈ {`text_delta`, `thinking_delta`, `signature_delta`, `input_json_delta`}
- `message_delta` carries `usage` (`output_tokens`, `input_tokens`, cache token counts) — the token-delta source.
- `assistant.message.content` blocks observed: `thinking`, `tool_use` (name=`Bash`/`Read`, with `input`), `text`.
- `user.tool_use_result` is a sibling of `message`: `{stdout, stderr, interrupted, isImage, noOutputExpected}` for Bash, `{type, file}` for Read.
- `result.success`: `total_cost_usd=0.0479`, `session_id=4fa68a55-…`, `num_turns=3`, `duration_ms=15329`, `stop_reason="end_turn"`, `terminal_reason="completed"`, `is_error=false`.

### Deltas from the prompt's assumptions (adapt to the capture, not the docs)

1. Partial deltas are **nested under `type=stream_event`** with an inner `event` (Anthropic SSE shape), not a flat "partial message delta" type. Token deltas come from `stream_event → message_delta.usage` and `content_block_delta`.
2. `rate_limit_event` is a **distinct top-level type** (the ApiRetry source). In this capture `rate_limit_info = {status:"allowed_warning", rateLimitType:"seven_day", utilization:0.92, surpassedThreshold:0.75, isUsingOverage:false}` — the live account is at **92% of its 7-day limit**. This is a real constraint on G4's "four concurrent sessions": a sustained 4-way live run risks crossing into overage/metered territory. Sizing G4 against this is a judgment call surfaced to the user.
3. `system` has subtypes `init` / `status` / `post_turn_summary`. `post_turn_summary.status_category` is a natural phase signal.
4. `user` carries `tool_use_result` as a sibling to `message` (richer than the prompt's "tool_result blocks").
5. `assistant` emits `thinking` blocks (extended thinking) alongside `text`/`tool_use`.

## Gates (filled in as phases complete)

| ID | Result | Evidence |
|----|--------|----------|
| G1 Konjo walls | PARTIAL | `cargo fmt --check` clean. `cargo clippy --workspace --all-targets -- -D warnings` clean. `cargo test --workspace` green (18 binaries). Dead code 0 (`RUSTFLAGS="-W dead_code"`). Undocumented public APIs 0 (`RUSTDOCFLAGS="-D missing_docs"` on lopi-core). **Not measured on this host:** coverage (cargo-llvm-cov not on PATH), mutation, cognitive-complexity, audit, deny — these run in CI Wall 2. New code carries 15 parser unit tests + a 3-test robustness suite + golden tests in 3 languages; not claiming a coverage % I did not measure. |
| G2 Parser robustness | PASS | `crates/lopi-agent/tests/parser_robustness.rs`: feeds all 44 real capture lines + 18 adversarial (truncated/non-JSON/unknown-type/missing-field) lines through `parse_line` and `structured_events`/`log_line`. Zero panics, unknown types → `StreamEvent::Other`. 3/3 tests pass. |
| G3 Contract parity | PASS | The same `crates/lopi-core/tests/fixtures/agent_event_golden.json` (6 events) decodes with identical field values in Rust (`agent_event_golden.rs`), TypeScript (`parser.test.ts`), and Swift (`AgentEventGoldenTests` via `xcodebuild test`, 1/1). |
| G4 Web E2E | PASS (minimal-proof variant) | Per the user's quota decision (92% of 7-day limit), ran a scoped live proof instead of four sustained sessions. Started `lopi sail` on :3137, submitted real tasks, and captured the live WebSocket with a raw-socket client: **109 frames during one real task, 0 `demo-*` frames**, and all six new structured types present with real values — `tool_call {tool:"Bash", summary:"find src/ tests/ …"}`, `token_delta {output_tokens:193, …}`, `cost {cost_usd:0.0114, num_turns:1, session_id:"a4bdfe80-…"}`, `phase {phase:"requesting"}`, `api_retry`, `tool_result`. Histogram: log_line×23 phase×9 token_delta×7 tool_call×6 tool_result×6 turn_metrics×4 status_changed×4 cost×2 api_retry×2. The task reached `success`. Tile/two-pane side-by-side layout is exercised in the macOS capture (G5). |
| G5 macOS E2E | PASS (live) | `xcodegen generate` + `xcodebuild build` clean; `xcodebuild test` green (1/1). Launched the built `Lopi.app` against a live `lopi sail` on :3000 with real tasks; captured the window via `screencapture -l<window-id>`. `artifacts/screenshots/04-macos-forge-live.png` (single pane, jade success orb) and `05-macos-forge-multipane.png` (2×2 grid: jade=success, rose=failed, grey=active orbs, per-pane goal/status/log). Panes update from real events; honest offline/empty banner added. |
| G6 Compliance | PASS | env-scrub present on all 3 spawn paths (`claude.rs`, `claude_stream.rs`). Diff grep: no new `ANTHROPIC_API_KEY`/api_key reads, no auto-merge, no token forwarding. `allow_self_modify` is `bool` with `#[serde(default)]` → false. |
| G7 Cost guard | PASS | `ClaudeCode::with_max_turns`/`with_max_budget_usd` emit the flags on the streaming spawn; the runner wires `--max-turns` from its turn cap + an optional `cli_budget_usd`. Verified live: `claude -p --max-turns 1` on a multi-step goal returned `subtype=error_max_turns`, `terminal_reason=max_turns`, `is_error=true` (cost $0.03). `StreamEvent::log_line` surfaces a `⛔ halted (error_max_turns)` line; unit tests pin this exact behavior. |

## Screenshots (in `artifacts/screenshots/`, delivered separately — not committed)

- `01-test-results.png` — Rust 902 / Web 264 / Swift 1 passing; G2/G3/G6/G7 summary; live-WS histogram.
- `04-macos-forge-live.png` — macOS Forge, single pane, live success session.
- `05-macos-forge-multipane.png` — macOS Forge 2×2 multipane, orbs colored by real state.
- `02-forge-multipane-demo.png` / `03-forge-offline-honest.png` — web Forge `?demo=1` vs honest offline state.
