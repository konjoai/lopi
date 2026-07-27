# lopi fuzz targets

Sprint S12, Phase 2. Three `cargo-fuzz`/`libfuzzer-sys` targets over the parsers that eat
attacker-influenceable input, identified because there was no fuzzing infrastructure in this
repo at all before this sprint (`grep`-confirmed: no `fuzz/`, no `proptest`/`arbitrary`/
`libfuzzer-sys`/`quickcheck` in any `Cargo.toml`).

| Target | Parser under test | Input source |
|---|---|---|
| `jsonrpc_response_fuzz` | `lopi_mcp::jsonrpc::Response` (`crates/lopi-mcp/src/jsonrpc.rs`, 233 LOC) | MCP server stdio replies — unvetted servers even after the S10 Phase 5 allowlist, since the allowlist pins *which binary* runs, not what it returns |
| `claude_events_fuzz` | `lopi_agent::claude_events::parse_line` (`crates/lopi-agent/src/claude_events.rs`, 483 LOC) | The `claude` CLI's `--output-format stream-json` stdout — agent output ultimately derived from repository content the operator doesn't fully control |
| `github_webhook_fuzz` | `lopi_webhook::github::fuzz_parse_and_extract` (`crates/lopi-webhook/src/github.rs`, 266 LOC) | GitHub webhook bodies, exercised pre-HMAC-verification for parse purposes only |

This directory is a **separate, detached Cargo workspace** (`fuzz/Cargo.toml` ends in a bare
`[workspace]` table) on purpose: `libfuzzer-sys` requires nightly, and this crate must never be
picked up by `cargo build --workspace`/`cargo test --workspace` on stable, which is what every
other gate in this repo runs against. Confirm with `cargo metadata` from the repo root that
`lopi-fuzz` does not appear as a workspace member.

## Running locally

```
cargo install cargo-fuzz
cd fuzz
cargo +nightly fuzz run jsonrpc_response_fuzz -- -max_total_time=60
cargo +nightly fuzz run claude_events_fuzz -- -max_total_time=60
cargo +nightly fuzz run github_webhook_fuzz -- -max_total_time=60
```

`fuzz/corpus/<target>/` is committed so runs are reproducible across machines/CI. The
`claude_events_fuzz` corpus is seeded from `artifacts/STREAM_CAPTURE.jsonl`, a real captured
`stream-json` session, split one line per file (the brief's own suggestion — "seeded from real
captured input").

## CI wiring

`.github/workflows/konjo-gate.yml`'s `fuzz` job runs all three targets for 60 seconds each on
every PR — short deliberately, per the brief: "A fuzz gate that adds ten minutes to every PR
gets disabled, which is the S10 anti-goal in a different costume." A longer scheduled campaign
(a nightly/weekly cron with a multi-minute budget per target) is not wired yet — a natural
follow-up once the PR-time job is confirmed stable, not built speculatively here.

## Honesty about verification status

These three harnesses were authored against the real parser APIs (each function signature was
read from source, not guessed) and reasoned through carefully, but **could not be compiled or
run** in the environment that wrote them — no nightly Rust toolchain, no `cargo-fuzz` binary,
and no `crates.io` network access were available there. The CI job that runs them
(`.github/workflows/konjo-gate.yml`'s `fuzz` job) is marked `continue-on-error: true` for
exactly this reason, with the plan to drop that flag and make it a hard gate once the first
real CI run confirms the harnesses actually compile and run clean. Any crash a real run finds
becomes a committed regression test *before* the fix, so the test demonstrably fails first
(the G-CAN-FAIL discipline the sprint brief asks for) — see `.konjo/killtests/S12/`.
