# KT-S12.2 — fuzz the three parsers (infrastructure, not a completed run)

**Sprint:** S12, Phase 2 · **Verdict:** infrastructure added; **NOT run in this environment** —
recorded honestly, not glossed over.

**Files under test:** `crates/lopi-mcp/src/jsonrpc.rs` (233 LOC), `crates/lopi-agent/src/claude_events.rs`
(483 LOC), `crates/lopi-webhook/src/github.rs` (266 LOC). Harnesses: `fuzz/fuzz_targets/*.rs`.

## What this sprint actually did

1. Confirmed the brief's premise: no fuzzing infrastructure existed anywhere in the repo (no
   `fuzz/`, no `proptest`/`arbitrary`/`libfuzzer-sys`/`quickcheck` in any `Cargo.toml`).
2. Added `fuzz/` as a detached `cargo-fuzz` workspace with three targets, each calling the real
   production parser function (not a reimplementation) — see `fuzz/README.md` for the exact
   API each target exercises and why.
3. Seeded `fuzz/corpus/claude_events_fuzz/` from the real captured session
   `artifacts/STREAM_CAPTURE.jsonl` (44 lines, one seed file per line), and hand-wrote a small
   representative seed corpus for the other two targets (valid + edge-case JSON-RPC responses,
   valid GitHub webhook payload shapes).
4. Added a synchronous `pub fn fuzz_parse_and_extract` to `crates/lopi-webhook/src/github.rs`
   so the webhook target can exercise the real field-extraction logic (`handle`/
   `dispatch_event`'s `.get()` chains) without needing a full async `TaskQueue`/server — see its
   doc comment for why it exists and what it's for.
5. Added unit tests (`crates/lopi-webhook/src/github_tests.rs`:
   `fuzz_parse_and_extract_does_not_panic_on_malformed_json`,
   `fuzz_parse_and_extract_accepts_well_formed_payloads`) that lock in the entry point's
   no-panic behavior against a hand-picked set of malformed shapes, runnable under the normal
   `cargo test` gate (not `cargo-fuzz`) so at least *some* coverage of this path runs on every
   PR even before the fuzz job itself is trusted.
6. Wired a `fuzz` job into `.github/workflows/konjo-gate.yml` — 60s per target, PR-only, per
   the brief's "short CI run, not a long one."

## What this sprint could not do, and why

The environment these harnesses were written in has **no nightly Rust toolchain, no
`cargo-fuzz` binary, and no `crates.io` network access** (`curl` to `crates.io` returns 403
through the environment's proxy). `rustup toolchain list` shows only `stable`. This means:

- The three fuzz targets were never actually compiled here.
- No fuzz run — not even a single iteration — has happened against them.
- The CI job that runs them is deliberately marked `continue-on-error: true`, with a comment
  explaining exactly this, so a compile failure on the first real run degrades to a visible
  warning rather than blocking every future PR on infrastructure nobody has verified.

Everything that *could* be verified without those three missing pieces was: every function
signature the harnesses call was read from source (not guessed), the wrapper added to
`github.rs` was compiled and unit-tested under the normal `stable` toolchain (see step 5 above,
which passed: `cargo test -p lopi-webhook fuzz_parse_and_extract` — 2 passed), and the fuzz
crate's `Cargo.toml` was checked against `cargo metadata`/`cargo build --workspace` to confirm
it does not leak into the main workspace.

## Verdict

**Infrastructure exists and is reasoned-through; the actual fuzzing has not happened yet.** The
honest state is: three targets, a seeded corpus, and CI wiring are in place and ready for their
first real run — which will either confirm them clean or (more likely, being untested) surface
a compile error to fix. Claiming this kill-test as fully "PASS" would misrepresent what was
actually done. Follow-up: the first CI run of the `fuzz` job on this PR (or the next one) is
the actual verification step; once it's green, drop `continue-on-error` per `fuzz/README.md`'s
own note and this file should be updated to record the real result.
