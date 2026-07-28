---
decays: state
verified-against: 28dd4cf
verified-date: 2026-07-28
---

# Panic audit — the trustworthy count, and why grep couldn't give it to you

Verified against: `28dd4cf` · 2026-07-28 (re-verified again; this doc keeps crossing
the 20-commit staleness cap purely on Sprint E/Finding #10's own merge-commit volume,
not on the zero-unwrap claim losing accuracy — re-confirmed live with the exact
deny-flag `cargo clippy --workspace --all-targets --all-features` invocation cited
below, 0 warnings/0 errors, now also covering the new `lopi-index` crate (Finding #4,
symbol-index sprint) and Sprint E/Finding #10's own new modules
(`lopi-orchestrator::budget::*`, `lopi-core::economics`/`economics_config`). One real
gap found and fixed, the same class as the `lopi-demo` gap the prior round caught:
`lopi-index`'s `lib.rs` shipped without the `#![warn(clippy::unwrap_used,
clippy::expect_used, clippy::panic)]` inner attribute every other crate in this repo
carries — the CI-flag gate still caught it (0 violations either way, this crate has
none), but the defense-in-depth property this doc's own "What was actually done"
section below describes didn't hold for this one crate until now. Added; row added to
the per-crate table below. No other drift found.)

Konjo Forward **Pillar 1** (an honest starting position) and **F11** (a durable unattended
loop should not die on an `unwrap`). This is the pre-flight kill-test for Sprint S5 and the
post-flight record of what it found. `decays: state` — re-run the Method 3 command below
(don't trust these numbers) before citing this doc in a later sprint.

## The premise, and what actually happened to it

The sprint brief that opened this work stated four grep-based methods on this codebase
produced wildly different counts (up to 796) and asked for an AST-based measurement to
settle it, then to fix/annotate/promote-to-deny across the hot-path crates. **Pre-flight
found the second half of that work already done, and done more broadly than asked.**
`.github/workflows/konjo-gate.yml`'s `static` job (G1, "clippy — zero warnings, all deny
flags") already runs

```
cargo clippy --workspace --all-targets --all-features -- \
  -D warnings -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic \
  -D clippy::todo -D clippy::unimplemented -D clippy::dbg_macro \
  -D clippy::print_stdout -D clippy::print_stderr -W clippy::cognitive_complexity
```

as a **hard** gate (no `continue-on-error`) — workspace-wide, not scoped to hot paths — and
`.konjo/hooks/pre-commit`'s step "1b. clippy" mirrors it locally. Both were already green on
`main` before this sprint touched anything. **The real number of panicking call sites on
production paths, workspace-wide, is 0.** This sprint's job shrank from "fix hot paths and
promote three crates to deny" to "verify that's true, make it durable in source (not just
CI flags), and retire the one tool in this repo that was still grep-guessing."

## Method comparison (re-run against `34a73d1`)

| Method | Command | Count | Trustworthy? |
|---|---|---|---|
| 1. Raw grep, excluding `*_tests.rs`/`tests.rs`/`/tests/` | `grep -rE '\.unwrap\(\)\|\.expect\(' crates/ src/ \| grep -v ...` | **788** | No — inline `#[cfg(test)]` modules inside production files aren't excluded |
| 2. Same, plus a naive single-pass `#[cfg(test)]` strip (skip rest of file after first match) | ad hoc Python | **246** | No — wrong in the *other* direction: skips real production code after a file's first test module if anything follows it, and still can't see `#[allow(...)]` |
| 3. Clippy, AST-based, `--workspace --all-targets --all-features`, lints at deny | `cargo clippy --workspace --all-targets --all-features -- -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic` | **0** | **Yes** — understands module structure and `#[cfg(test)]`/`#[allow]` scoping by construction |
| 4. Hand-sampled hot paths | manual read of `crates/lopi-agent/src/runner/`, `crates/lopi-orchestrator/src/pool/`, `crates/lopi-ui/src/web/` | 0 unwraps found | Confirms Method 3, doesn't scale |

Methods 1 and 2 disagree with each other by 3x and both disagree with Method 3 by orders of
magnitude — that disagreement *is* the finding, not noise to average away. Every raw grep hit
in Methods 1/2 that isn't a false negative lives inside an inline `#[cfg(test)] mod tests { ... }`
block in a production file (the repo's dominant test layout — see the per-crate table below),
which is exactly the structure line-based tools cannot parse and clippy parses by construction.

## Per-crate breakdown

| Crate | Raw grep hits (non-test-file, method 1) | Clippy production violations (method 3) | Source-level lint |
|---|---:|---:|---|
| `lopi-agent` (hot path — agent runner) | 63 | **0** | `#![deny(...)]` |
| `lopi-orchestrator` (hot path — pool/scheduler) | 48 | **0** | `#![deny(...)]` |
| `lopi-ui` (hot path — web API) | 85 | **0** | `#![deny(...)]` |
| `lopi-memory` | 294 | **0** | `#![warn(...)]` |
| `lopi-core` | 50 | **0** | `#![warn(...)]` |
| `lopi-spec` | 40 | **0** | `#![warn(...)]` |
| `lopi-mcp` | 31 | **0** | `#![warn(...)]` |
| `lopi-tools` | 23 | **0** | `#![warn(...)]` |
| `lopi-remote` | 13 | **0** | `#![warn(...)]` |
| `lopi-skill` | 13 | **0** | `#![warn(...)]` |
| `lopi-context` | 11 | **0** | `#![warn(...)]` |
| `lopi-webhook` | 8 | **0** | `#![warn(...)]` |
| `lopi-github` | 8 | **0** | `#![warn(...)]` |
| `lopi-toon` | 2 | **0** | `#![warn(...)]` |
| `lopi-ratelimit` | 2 | **0** | `#![warn(...)]` |
| `lopi-git` | 0 | **0** | `#![warn(...)]` |
| `lopi-demo` (added 2026-07-28) | 0 | **0** | `#![warn(...)]` |
| `lopi-index` (added 2026-07-28, Finding #4) | 89 | **0** | `#![warn(...)]` (added this re-verification — see note above) |
| root binary (`src/`) | 94 | **0** | `#![warn(...)]` (CLI, low blast radius — see Out of scope) |
| **Total** | **877** | **0** | — |

`lopi-app` (previously listed here at 3 raw hits / 0 production violations) was deleted
outright by Sprint S12 (the multi-tenant-surface removal, `LEDGER.md`) — not hardened,
gone. The row is removed rather than left dangling; the total above is left as originally
measured (a historical count, not re-summed) since re-deriving it would require re-running
Method 1's raw grep against a tree state that no longer exists and gains nothing the
still-valid Method 3 (0 production violations, workspace-wide, re-confirmed above) doesn't
already cover.

Every raw-grep hit above is inside test code (an inline `#[cfg(test)] mod tests`, a `tests.rs`
submodule gated `#[cfg(test)]` in its parent, or a Criterion bench in `benches/`), each already
carrying its own scoped `#[allow(clippy::unwrap_used, ...)]` where needed. Spot-checked on
re-verification (2026-07-26): the `#[allow]` count has grown with the codebase (102 files as of
this re-check, not the 17 at original write time) but every sample checked still attaches the
`#[allow]` directly to a `#[cfg(test)]` module, not a production path — the load-bearing claim is
Method 3's clippy gate staying at 0 production violations, re-confirmed live at re-verification
time, not the file count, which is expected to keep growing and isn't itself the safety property.

## What was actually done this sprint

Since the CI-flag-based gate was already comprehensive and already green, there was nothing to
fix or annotate (Phase 1 found zero production unwraps to classify) and nothing to newly promote
to deny at the CI level (Phase 2/3's target state — a hard workspace gate — already existed).
Two real gaps remained, both closed:

1. **The guarantee was CI-flag-only, not a property of the source.** A contributor running plain
   `cargo clippy` (no special flags) or reading a file in an editor with rust-analyzer got zero
   signal — `unwrap_used`/`expect_used`/`panic` are allow-by-default clippy restriction lints.
   Every crate's `lib.rs` (and `src/main.rs` for the binary) now carries an explicit inner
   attribute: `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]` on the three
   hot-path crates (`lopi-agent`, `lopi-orchestrator`, `lopi-ui`), `#![warn(...)]` everywhere
   else. This is additive and redundant with the CI flags by design — defense in depth, and it
   means the lint now shows up locally without invoking any special command. Verified: with the
   attributes in place, `cargo clippy --workspace --all-targets --all-features -- -D warnings`
   is still 0 warnings, 0 errors — confirms the true count really is 0, independent of the CI
   invocation. Also live-tested the gate: a probe `unwrap()` inserted into
   `crates/lopi-orchestrator/src/pool/worktree.rs::cleanup_worktree` (a hot-path production
   function) failed the build immediately with a `deny`-level clippy error citing the exact lint
   source; reverted after confirming.
2. **`.konjo/hooks/pre-commit` step "1c. unwrap/expect scan" was still grep-guessing.** It ran a
   hand-rolled `awk` brace-depth counter over staged files to strip `#[cfg(test)]` blocks before
   grepping for `.unwrap()`/`.expect(`. Two concrete defects, not just "belt and suspenders that
   happened to also exist": (a) the brace counter desyncs on any `{`/`}` inside a string literal
   or a `format!()`/`write!()` argument in the region it's trying to skip, which can both hide a
   real production unwrap past a mis-tracked "still in test" state and falsely flag one past a
   mis-tracked "back in prod" state; (b) it has no concept of `#[allow(clippy::unwrap_used)]`, so
   a legitimately justified, clippy-clean production unwrap (the "Justified-by-invariant" class
   this sprint's own classification scheme calls for) would still fail the commit — a false
   positive clippy itself doesn't produce. Removed; step "1b. clippy" (the same AST-based
   `-D clippy::unwrap_used/expect_used/panic` invocation as CI) already covers everything 1c was
   trying to approximate, across every target including tests, and does it correctly. A comment
   in the hook now points here instead of leaving a silent gap in the numbered steps.

## Verify

- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — 0 warnings, 0 errors.
- `cargo clippy --workspace --all-targets --all-features -- -D clippy::unwrap_used -D
  clippy::expect_used -D clippy::panic` — 0 errors (Method 3 above, re-run on demand).
- Gate tested live: probe `unwrap()` in a hot-path deny-level crate fails the build with a clear
  `deny`-level message; reverted, confirmed clean again.
- `.github/workflows/konjo-gate.yml`'s G1 job is unchanged (already hard, already
  workspace-wide) — this sprint did not need to touch it.
