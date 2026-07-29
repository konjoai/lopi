# Next Session Prompts

Paste the relevant entry as the first message of a fresh Claude Code session in
the `lopi` repo. Newest first.

---

## Next Session -- after Track C (error-taxonomy ratchet + lopi-git migration, `[0.40.0]`)

Track C shipped the gate S13R's error-taxonomy migration never got: a per-crate
`anyhow::`-usage ratchet (`.konjo/error-taxonomy.txt` + `error_taxonomy_check.py`,
wired hard into `konjo-gate.yml` and registered in `.konjo/profile.yml`'s `gates:`
with a real kill-test), and finished converting `lopi-git`'s three remaining files
(`manager.rs`, `rebase.rs`, `worktree.rs`) off `anyhow::` onto typed errors. Read
first: `CHANGELOG.md`'s `[0.40.0]` entry, `LEDGER.md`'s "Track C" entry.

**What's already done and should not be re-derived:** `.konjo/error-taxonomy.txt`
holds a locked floor row for all 18 crates under `crates/` (seeded from a real
measurement, not the S13R brief's carried-forward numbers -- `lopi-core` measures 1,
not 2, see `LEDGER.md` for why); `error_taxonomy_check.py` kill-tested (KT-C.1)
against both a planted regression in an already-migrated crate (rejected) and an
unchanged unmigrated crate (accepted) in the same run
(`.konjo/scripts/test_error_taxonomy_killtest.sh`); `lopi-git` is now fully migrated
(`GitManagerError` in `manager.rs`, shared by `rebase.rs`; `WorktreeError` in
`worktree.rs`; both exported from `lopi_git`'s crate root) and its `anyhow` dependency
dropped from `Cargo.toml`; `lopi-git`'s error-taxonomy floor row is now 0.

**First thing to do:** confirm `Cargo.toml`'s version (`0.40.0`) still matches
`CHANGELOG.md`'s top entry before touching anything.

**Carried forward, explicitly not silently dropped -- pick this up rather than
starting something new:**

1. **`lopi-memory`'s error taxonomy** -- 0 of 30 `anyhow` files converted (floor row
   unchanged at 30). This was explicitly optional for Track C (deferred per that
   track's own brief: "the ratchet itself is the deliverable, migration progress is a
   bonus") and still needs its own dedicated session, not a squeeze-in. Look for a
   natural seam first -- the `store/` module's ~30 sub-files (one file per table,
   `crates/lopi-memory/src/store/*.rs`) likely share a shape (a `sqlx::Error` source,
   maybe a `serde_json` parse source) that argues for one shared `MemoryStoreError`
   enum rather than 30 bespoke ones; check whether `sqlite_pool.rs`'s
   `SqlitePoolError` in `lopi-core` (the dual-pool connection-open error) is the right
   thing to wrap or compose with, since `lopi-memory` almost certainly calls into it.
   Convert a handful of files, run `cargo test -p lopi-memory` plus a full
   `cargo build --workspace`, then lower `.konjo/error-taxonomy.txt`'s `lopi-memory`
   row to match in the same commit -- never migrate files without also lowering the
   floor, and never lower the floor without the migration backing it up.
2. **Doc-link debt** (the rustdoc gate, still soft, named owner "whichever sprint
   next touches `lopi-agent`/`lopi-orchestrator`/`lopi-mcp` docs," target "before
   Sprint S14 closes" -- unchanged by this track). Re-run
   `RUSTDOCFLAGS="-D missing_docs -D rustdoc::broken_intra_doc_links" cargo doc
   --workspace --no-deps` to get the current exact list before starting.
3. **`gate_polarity`'s one filed real defect**: `eval_runner.rs:29`'s
   `evaluate_acceptance_gate` proceeds when no `Acceptance` is configured -- unchanged
   by this track, still needs the explicit opt-in redesign
   `verifier_error_proceeds(fail_open: bool)` already got.

**Non-goals, correctly not attempted:** migrating all 18 crates; touching the
indexing or coverage ratchet floors/checkers; anything referencing S14 or later
sprint work.

---

## Next Session — after Sprint S13R (Phases A–F, `[0.39.0]`)

Sprint S13R connected lopi to kiban v1.8.0 (Phase A), re-ran the Phase 0 audit and
cleared its stop rule at 0 unmapped claims (Phase B), then resumed and completed the
original S13 Phases 1–4 (renamed C–F: determinism substrate, panic/resource surface,
error taxonomy, enforcement-from-first-prompt). Read first: `CHANGELOG.md`'s `[0.39.0]`
entry, `LEDGER.md`'s two S13R entries, and `.konjo/killtests/S13/PHASE0-STOP-RULE.md`'s
2026-07-29 append.

**What's already done and should not be re-derived:** kiban pinned at v1.8.0
(`.konjo/kiban.ref` + `KIBAN_REF` in both CI jobs that reference it);
`.konjo/profile.yml` wired via a new `konjo-gates` CI job; `CLAUDE.md` converted to the
Phase-13 section contract; `.claude/rules/security.md` split into
`security-invariants.md`/`security-sinks.md`; `rust-toolchain.toml` pinned to `1.88.0`
(real bisection, not guessed); `[workspace.lints]` + `lints.workspace = true` on all 18
crates + the root binary; `overflow-checks = true` in `[profile.release]` (KT-S13.2
verified) + `[profile.bench]` added; both production unbounded channels converted to
bounded; the indexing floor seeded at 211 (`.konjo/indexing-floor.txt`, re-measured —
does not match the brief's earlier "202", see `LEDGER.md` for why); the function-length
gate written and wired (`.konjo/function-length-ceiling.txt`, seeded at 74);
`lopi-core`'s error taxonomy fully converted to `thiserror`; a `SessionStart` hook added;
the hardcoded `/Users/wesleyscholl/lopi/` paths in `.claude/settings.json` and
`post-edit.sh` fixed; `post-edit.sh` extended to cover `web/` TypeScript/Svelte edits.

**First thing to do:** confirm `Cargo.toml`'s version (`0.39.0`) still matches
`CHANGELOG.md`'s top entry and `VERSION` before touching anything.

**Carried forward, explicitly not silently dropped — pick one and finish it rather than
starting something new:**

1. **`lopi-git`'s error taxonomy** — only `diff.rs` (1 of 4 `anyhow` files) converted.
   `manager.rs` (18 fallible fns), `rebase.rs` (4), `worktree.rs` (11) remain. Starting
   design sketch recorded in `LEDGER.md`'s `Error-Taxonomy-1`: a `GitManagerError`
   wrapping `git2::Error` + `std::io::Error` + a `CommandFailed { context, stderr }`
   variant for the `anyhow::bail!` shell-out sites (git push, rebase abort). Convert one
   file at a time, running that file's test suite plus a full `cargo build --workspace`
   after each — this crate is used everywhere (`lopi-agent`, `lopi-orchestrator`), so a
   rushed multi-file conversion in one pass is exactly the risk this note exists to head
   off.
2. **`lopi-memory`'s error taxonomy** — 0 of 30 `anyhow` files converted. The largest
   remaining piece of Phase E by far. Needs its own dedicated session, not a squeeze-in
   at the end of another sprint. Look for a natural seam (the `store/` module's
   sub-files likely share a shape) before picking a per-file or a unified-enum approach.
3. **Doc-link debt** (the rustdoc gate, kept soft this sprint with a named owner —
   "whichever sprint next touches `lopi-agent`/`lopi-orchestrator`/`lopi-mcp` docs" — and
   a target of before Sprint S14 closes). Re-run
   `RUSTDOCFLAGS="-D missing_docs -D rustdoc::broken_intra_doc_links" cargo doc
   --workspace --no-deps` to get the current exact list (it grew past what this
   sprint's own re-measurement found — `lopi-agent` 11, `lopi-orchestrator` 8,
   `lopi-mcp` 1 — by the time a future session gets to it) before starting; qualify
   each bare `[Type]`/`[func]` link with its full path, following `lopi-core`'s
   already-fixed precedent.
4. **`gate_polarity`'s one filed real defect**: `eval_runner.rs:29`'s
   `evaluate_acceptance_gate` proceeds when no `Acceptance` is configured — the same
   shape as the already-fixed `verifier_runner.rs`/`scorer.rs` sites, but deliberately
   not fixed this sprint since doing it properly means the same kind of explicit
   opt-in redesign `verifier_error_proceeds(fail_open: bool)` got, a real behavior
   decision (does an existing task with no acceptance start failing?), not a small
   patch.
5. **Report the `pricing.rs` false positive to kiban** if a future session gets push
   access there: `is_stale_given`'s `None => true` is flagged by `gate_polarity` as a
   permissive default, but `true` here means "flag as stale" — the restrictive,
   cautious answer, not a permissive bypass. A real blind spot in the engine's
   bare-literal heuristic (it assumes any literal `true`/`1.0`/`Ok(())` is inherently the
   permissive end of range, which breaks for a boolean whose polarity runs the other
   way), worth a kill-test fixture in kiban itself.

**Non-goals, correctly not attempted:** squish/vectro's own CLAUDE.md conversions
(kiban's `Lopi-Gate-Reconciliation-1` explicitly defers these, lopi was the named
pilot); `cargo-semver-checks`/`cargo-udeps`/flake budget/fuzz-in-CI (still S14 per the
original S13R brief); wiring the Konjo Verifier onto the `sail` path (still Wave 1/F1).

---

## Next Session — Resume Sprint S13 at Phase 1, after the Phase 0 stop (`[0.38.0]`)

Sprint S13 (Quality Substrate and Continuous Enforcement) ran Phase 0 (the
honesty pass on quality claims) and then **stopped**, per the sprint's own
stop rule: Phase 0 found 5 self-claims with no genuine enforcing step
(2 dark `.konjo/rubrics/*.toml` files with zero code consumer, 3 of
`CLAUDE.md`'s 8 "Additional Hard Rules" bullets that were actually soft
`continue-on-error` gates or had no mechanical check at all) against a
threshold of 3. Full audit: `.konjo/killtests/S13/PHASE0-STOP-RULE.md`.
One-way-door decisions: `LEDGER.md`'s S13 Phase 0 entry.

**What's already done and should not be re-derived:** the two dark rubrics
(`refactor_safety.toml`, `security_audit.toml`) are deleted and every doc
that claimed "three canonical rubrics ship" (`KONJO_VERIFIER.md`, `PLAN.md`)
is corrected; the 3 false-hard `CLAUDE.md` bullets (coverage 80%/95%, doc
coverage, function length) now correctly describe their real enforcement
status, each cited to its exact `konjo-gate.yml` job:step; 4 dead path
globs in `.claude/rules/benchmarking.md` and `.claude/rules/testing.md` are
fixed and verified against the real repo layout; the sprint brief's baseline
evidence table is re-verified with two real drifts corrected (a second
production unbounded channel at `src/repl/mod.rs:76` the original baseline
missed, and `anyhow::` usage grown from 106 to 131 files).

**First thing to do:** before writing any Phase 1 code, re-verify Phase 0's
claim-mapping table is still accurate (nothing should have drifted since —
this repo's `main` doesn't move fast enough for that — but the brief's own
discipline is "verify, don't assume"). If it still holds at ≤3 unmapped
claims, proceed straight to Phase 1: `rust-toolchain.toml`, MSRV bisection
(record the bisection result, don't guess), the edition/`unsafe`-2024
question in `crates/lopi-ui/src/client/auth.rs` (resolve why CI is currently
green before touching it), `[workspace.lints.rust]`/`[workspace.lints.clippy]`
on the root `Cargo.toml` + `lints.workspace = true` on all 18 crates, and
`overflow-checks = true` in `[profile.release]` — gated by **KT-S13.2**
(add a temp `#[test]` in `lopi-core` doing `u64::MAX + 1` via a runtime
value, confirm it panics under `--release` only after the profile change,
then delete the temp test). Phase 1 is the first phase to introduce a real
gate, so **KT-S13.1** (a bad+good fixture pair under `.konjo/fixtures/S13/`
for every new gate, proven to reject/accept before it's wired into CI)
starts applying from here forward — it did not apply to Phase 0 since Phase
0 introduced no gates, only corrected/deleted existing claims.

Then Phase 2 (indexing floor ratchet — seed `.konjo/indexing-floor.txt` at
the current count, which needs re-measuring since Phase 0 found the
production-indexing-site count (202) unchanged from baseline but did not
re-verify it post-Phase-0-edits), Phase 3 (error taxonomy — `lopi-core`,
`lopi-git`, `lopi-memory` only, `LEDGER.md`-tracked), Phase 4 (the actual
enforcement-from-first-prompt wiring — `CLAUDE.md` security/resource lines,
`security.md` split into invariants/sinks, `SessionStart` hook, the
hardcoded macOS path in `.claude/settings.json`, `post-edit.sh` extension,
CI wiring for every new gate).

---

## Next Session — T1 (Input & Command Layer), after Sprint T0 (`[0.35.0]`)

Sprint T0 (TUI Client Foundation & Domain Port) landed at `0.35.0`. The TUI
now has a write-capable client (`lopi_ui::client::TuiClient` +
`RemoteClient`/`LocalClient`) and a single Rust port of the loop-stack
domain model (`lopi_core::stack`) — but **no new widgets were built**. T1 is
the first sprint that actually renders anything: an always-focused
`InputBar` widget plus a `:`-prefixed command palette with preset-alias
autocomplete off `lopi_core::stack::preset_catalog()`. Goal submission goes
through the TUI via `TuiClient::create_task`, using
`lopi_ui::client::stack_payload::pane_submit_payload` for the bare-prompt
case.

**Read first:** `CHANGELOG.md`'s `[0.35.0]` entry and `LEDGER.md`'s Sprint T0
section — both one-way doors (the `lopi-core`/`lopi-ui` split for the
domain port vs. wire-payload builders, and `RemoteClient` as authoritative
over `LocalClient`).

**Carrying forward from T0, so T1 doesn't have to re-derive it:**

- `LocalClient`'s chain methods (`list_chains`/`get_chain`/`create_chain`/
  `enable_chain`/`disable_chain`/`run_chain_now`) are stubbed to return
  `ClientError::Unsupported` — `ChainScheduleManager` isn't reachable
  outside the axum `AppState` today, and building a second one from
  `LocalClient`'s own `AgentPool`/`MemoryStore` would race the real one
  inside `lopi sail`. Not T1's problem (T1 doesn't touch chains at all),
  but T3 (Loop Stack Builder) will hit this directly and should re-verify
  the finding rather than assume it's still accurate.
- `LocalClient` has no real caller yet — `lopi watch --local` still
  constructs an empty, disconnected `EventBus`. If T1 (or any later sprint)
  wants an embedded-TUI-inside-`sail` mode, that wiring doesn't exist yet
  and is new scope, not something T0 half-built.
- KT-T0.3's fixture set (`crates/lopi-ui/src/client/stack_payload_tests.rs`
  — three cases lifted from `stack.test.ts`) is a permanent regression
  test, not a throwaway. T1 doesn't need to touch it (it doesn't add new
  `cardToTaskPayload` fields), but T2 (Prompt Loop Builder) and T3 should
  extend it rather than write a parallel fixture set when they port more of
  `stack.ts`.
- `AppState::cognition` (`crates/lopi-ui/src/tui/cognition.rs`) now retains
  the six previously-dropped event variants, but nothing renders it — T5
  (Live Cognition Surface) is still the first sprint to build that panel,
  not T1.

**Kill test for T1:** typing `:implement fix the thing` in the TUI attaches
the same eval set the web composer attaches for the same input — i.e. the
resulting `CreateTaskRequest.acceptance` matches what
`lopi_core::stack::preset_catalog()[&PresetKey::Implement].evals` compiles
to via `evals_to_acceptance`.

---

## Next Session — after the web-composer loop.toml sprint (`[0.32.0]`)

**The web composer's `autonomy` control is now wired end to end** —
`CreateTaskRequest.autonomy_level`, resolved against the repo's
`.lopi/loop.toml` when unset (file = base, request = override — the
one-way-door precedent every future loop-config field must follow, see
`LEDGER.md`). `no_progress_limit` and `isolation` (branch/worktree) are new
per-task overrides on the same contract. Budget preset (`quick|standard|
deep|unlimited` + USD) now has a real composer control wired to the
pre-existing `budget_override` field. Read first: `CHANGELOG.md`'s
`[0.32.0]` entry, `LEDGER.md`'s entry for this sprint (both one-way doors —
the precedence contract, and "no control renders unless it's wired or
correctly absent"), `UI_PLAN.md`'s Backend Bindings table (corrected this
sprint — several rows it called "Missing"/"Partial" were already done by
earlier sprints; re-verify against `types.rs`/`task.rs` before trusting any
row in it, this is now the second time it was found stale).

**What's still file-only, deliberately, not by oversight:** `vision_path`,
`trust_ceiling`, `self_prompt`, `skills_enabled`, `rules_enabled`,
`permission_allow` — structural/rarely-per-run, per this sprint's own scope
boundary. None of them render as composer controls (correctly — "not shown"
is honest for a field that isn't wired, per Pillar 1). If a future sprint
wants any of these in the composer, it needs its own scoping pass; this one
deliberately didn't touch them.

**What a future sprint should pick up:**
1. **Verifier gate has a backend field but no composer control.** `Task.
   verifier_required/verifier_model/verifier_effort` are all on
   `CreateTaskRequest` already (an earlier sprint), but nothing in the
   composer exposes them — `UI_PLAN.md`'s Gap Map named `VerifierToggle.svelte`
   and it still doesn't exist. Small, well-scoped, not touched this sprint
   (out of this sprint's stated scope).
2. **Surfacing the resolved config, not just accepting an override.** Phase 3's
   honesty rule was satisfied by "wired or correctly absent," but the sprint
   brief's stronger suggestion — showing the *effective* `.lopi/loop.toml`
   value next to an `auto`/inherit control (e.g. "Auto · from loop.toml (repo
   is actually verified_pr)") — wasn't built; it needs a live per-repo config
   fetch the composer doesn't currently make. The existing `/loop` Loop
   Engineering cockpit already displays a repo's effective config in full;
   consider reusing that fetch rather than inventing a second one.
3. **`GuardrailsPopover`'s stack-scope `budget` row was removed** (Phase 3
   honesty audit — it was inert, no backend or client consumer). If a
   genuine chain-level budget concept is ever wanted, it needs its own design
   (there's no server-side "whole chain" today — a chain is N independent
   task creations) before any control reappears at that scope; don't
   re-add the removed row without one.

---

## Next Session — after Sprint F4 (session continuity) — Sprint F5 (build cache) is next

**Sprint F4 gave one CLI session per attempt (plan → implement → fix, not
one cold spawn per phase), with a silent cold-spawn fallback on any resume
failure, structural checker isolation, and task-level correlation.** Read
first, in order: `CLAUDE.md`, `CHANGELOG.md`'s `[0.31.0] — Sprint F4` entry,
`LEDGER.md`'s `Sprint F4` entry in full (both one-way doors: the
per-attempt session lifecycle, and the checker-isolation guarantee made
structural), all five `.konjo/killtests/F4/KT-4.*.md` files, `benchmarks/
results/<ts>_f4_session/summary.md`, then this file's own words below.

**Version note:** F4 was developed against a HEAD where both F1
(`0.29.0`) and F3 (`0.30.0`) had already landed — the brief's own
`§Ordering` section anticipated this exact case ("F4 takes `0.30.0` if F1
lands first, `0.29.0` otherwise") but didn't anticipate F3 landing *first*
and also taking `0.30.0`. F4 takes **`0.31.0`**, the next free slot, rather
than either already-claimed number.

Sprint F5 (build cache) is next — the brief's own hand-off target. Four
things carry forward:

1. **KT-4.4's cache-boundary number is F5's own input.** F4 found the
   `claude` CLI defaults to Anthropic's **1-hour** prompt-cache tier
   (`ephemeral_1h_input_tokens`), not 5 minutes — measured directly from
   the usage envelope across every kill-test and benchmark call this
   sprint made, not inferred. This is *why* F4 shipped `implement → fix`
   continuity, not just `plan → implement`: the boundary turned out to be
   an order of magnitude wider than the brief's own worry assumed. **F5
   should read this as: your build-cache speedup widens F4's viable window
   further, it doesn't rescue a narrow one.** If F5's own measurement ever
   shows a real `cargo test` phase taking close to an hour on a cold
   worktree, that is the point F4's `implement → fix` continuity would
   start losing its cache benefit — check the actual gap F5 measures
   against KT-4.4's ~1-hour figure before assuming continuity still holds
   at whatever speed F5 lands at.
2. **The Phase 5 measurement in this sprint is NOT the full 30-run
   T01–T10 corpus the brief's own gate asked for.** `benchmarks/corpus/
   README.md`'s "Status (Sprint F0, 2026-07-26)" note is still accurate —
   that corpus run has never happened, and is now blocking F4's own
   headline hypothesis in addition to F0/F1's prior notes below. What F4
   *did* run: a small (n=10), real (not synthetic — this session had live
   `claude` CLI access, unlike prior sessions' recorded constraint) paired
   sample directly measuring `plan → implement` cost/cache-share under
   cold vs. resumed conditions on a scratch repo, not this repo's own
   corpus. See `benchmarks/results/<ts>_f4_session/summary.md` for the
   real numbers and the honest scope limit — do not read this as
   confirming the brief's own 30-run/T01–T10 merge gate; that gate is
   still open. Whoever eventually runs the T01–T10 corpus (now blocking
   F0's own Phase 3, F1's Phases 5-6, and F4's own merge gate at once)
   should treat F4's small-sample finding as a strong prior to check
   against, not a substitute for the real run.
3. **The per-attempt session id (not the raw `TaskId`) is now the shape
   any future correlation work should build on.** `tasks.cli_session_id`
   holds the *most recent attempt's* session id, matching
   `tasks.branch`/`tasks.repo`'s existing "most recent attempt only"
   precedent — see `KT-4.2.md` for why the raw `TaskId` doesn't work
   here (it's stable across retries; CLI sessions are not).
4. **F1's own checker-isolation guarantee is now structural, not just
   conventional** — `verifier_cli.rs`/`postmortem_cli.rs` pass
   `SessionMode::None` explicitly through `apply_cli_caps`'s new `session`
   parameter, and both existing negative tests were extended to also
   assert no `--session-id` (not just no `--resume`). Any future sprint
   that adds a shared "give this spawn a session" helper must default
   these two call sites' `SessionMode::None` explicitly, not inherit
   whatever a generic helper does by default.

Also worth knowing, not blocking: KT-4.1 found `--permission-mode
bypassPermissions` itself fails in this sandboxed (root) container — the
same class of finding as F1's KT-1.3 on `--bare`. Every live call this
sprint substituted `acceptEdits` to verify the actual resume mechanics
instead. Whoever next has a real, non-root machine with a logged-in
`claude` CLI should re-run KT-4.1's specific `bypassPermissions` check.

---

## Next Session — after Sprint F3 (decouple log persistence) — Sprint F4 (session continuity) is next

**Sprint F3 decoupled the event bridge's live broadcast from `task_logs`
persistence.** Read first, in order: `CLAUDE.md`, `CHANGELOG.md`'s
`[0.30.0] — Sprint F3` entry, `LEDGER.md`'s `Sprint F3` entry (the
best-effort-persistence one-way door), all four
`.konjo/killtests/F3/KT-3.*.md` files, `benchmarks/results/
20260726T205826Z_f3_bridge/summary.md`, then this file's own words below.

**Version note:** F3 was developed against `6688d7d` (post-F2, `0.28.0`),
before F1 had landed, and its own draft notes assumed it would take
`0.29.0`. **F1 landed first instead** while F3's PR was in review (see the
`Sprint F1` entry immediately below this one) and took `0.29.0` — so F3
takes **`0.30.0`**.

Sprint F4 (session continuity) is next. Three things carry forward, two
from F3 and one inherited through F3 from F1's own handoff below:

1. **`KT-3.4`'s volume estimate was made pre-F1; F1 has since landed, but
   F3's *measurement* doesn't need a re-run — only a future live
   measurement does.** F3's 30-run paired benchmark
   (`benchmarks/results/20260726T205826Z_f3_bridge/`) uses a synthetic-load
   harness (`event_bridge_bench.rs::bridge_load_bench`) that injects
   `AgentEvent::LogLine` traffic directly onto the bus at a documented,
   stated rate (~250 lines/sec/agent, 4 synthetic agents) — it never
   exercises F1's verifier/judge/post-mortem code, so F1 landing changes
   nothing about what that harness measures. What F1 *does* change is the
   real production event rate a **live** (non-synthetic, real-agent)
   re-measurement would see. That re-measurement was never attempted in F3
   (out of scope, and this session's environment can't run one anyway —
   see `KT-3.1`'s own environment-substitution note) but is now actionable
   rather than merely estimated, since F1's verifier-session code exists to
   profile. Whoever eventually runs a real four-agent M3 measurement of the
   bridge should account for F1's added verifier-session traffic in the
   observed rate, rather than assuming F3's synthetic rate still applies.
2. **The durability one-way door — log persistence is now best-effort under
   pressure.** `LEDGER.md`'s F3 entry: under sustained overload, the
   bridge's persistence channel drops `task_logs` rows (counted, not
   silent — `lopi_task_log_persist_dropped_total` at `/metrics`) rather
   than blocking the live stream or growing unbounded. This was confirmed
   safe by `KT-3.3`: nothing reads `task_logs` for correctness, replay, or
   gating today — only display surfaces (web dashboard tail, Telegram
   `/tail`, MCP `lopi_get_logs`, `lopi diag` export). **If session
   continuity work (F4) or any later sprint starts treating `task_logs` as
   more than a display/inspection surface** — e.g. reconstructing session
   state from logged lines, or feeding an evidence bundle that needs
   completeness guarantees — **that is the trigger to reopen this door**
   and add backpressure or durable buffering back explicitly, rather than
   assuming today's best-effort behavior already covers it.
3. **F1's own item 3 below (F4 must not resume the checker session) still
   applies and is worth reading in full** — it's about a different
   subsystem (`verifier_cli.rs`/`postmortem_cli.rs`'s session isolation)
   than F3 touched, but it's the same kind of constraint: a fresh-session
   guarantee that a future session-continuity helper could accidentally
   erase if it's applied blanket-wide instead of call-site-by-call-site.

## Next Session — after Sprint F1 (The Verifier Is Real) — F9 unblocked; three carried-forward items

**Sprint F1 gave the verifier, the judge tier, and post-mortem a CLI backend
(`claude -p` on subscription auth), so all three actually run in every real
deployment instead of silently no-op'ing forever (`with_api` was never wired in
production — confirmed again by this sprint's own `grep`).** Phases 1–4 shipped;
Phases 5 (two-tier checker) and 6 (`--append-system-prompt` on the worker) did not —
both are gated on a corpus measurement this session couldn't run, per the brief's own
"ship without it" fallback. Read first, in order: `CHANGELOG.md`'s `[0.29.0] —
Sprint F1` entry, `LEDGER.md`'s Sprint F1 entry, all four `.konjo/killtests/F1/KT-1.*.md`
files, then this file's own words below. F1 was this repo's Sprint F1; Sprint F2
(Correctness Holes) already landed independently before F1 did — see `CHANGELOG.md`'s
`[0.28.0]` entry — so the two are not in the numeric order their briefs implied.

Four things carry forward:

1. **KT-1.3's `--bare` finding needs re-verification on a real (non-sandboxed) target
   machine.** In this session's own container, `--bare` failed CLI authentication
   6/6 times, reproducibly — `claude --help` documents it as skipping "keychain
   reads," and this container's credential wiring appears to depend on one. The
   checker ships without `--bare` as a result. **This may be specific to this
   sandboxed session's credential proxying and not a general property of a real
   user's subscription login** (which more commonly reads a plain on-disk token
   under `~/.claude/`, not a keychain). Whoever next has access to a real machine
   with a logged-in `claude` CLI should re-run KT-1.3's `--bare` vs. non-`--bare`
   paired comparison for real. If `--bare` turns out to work there, re-add it to
   `verifier_cli.rs`/`postmortem_cli.rs` for the cost/latency win KT-1.2 documented
   (a non-`--bare` checker session loads hooks/`CLAUDE.md`/skills and can burn real
   turns/spend on tool-discovery detours before concluding it's denied) — do not
   assume the auth failure generalizes without checking, but do not assume it's
   safe to add `--bare` back without checking either.
2. **The T01–T10 corpus run is still outstanding — now blocking two sprints'
   worth of work, not one.** F0's Phase 3 flagged it as attended, hardware-required,
   and not runnable in an unattended session; F1's Phase 5 (two-tier checker cost/
   agreement measurement) and Phase 6 (worker system-prompt pass-rate measurement)
   both needed it too and both went unshipped as a result, per each brief's own
   "measure first, or don't ship" instruction. Whoever picks this up should treat one
   corpus run as unblocking three sprints' pending measurements at once, not run it
   three separate times. Follow `benchmarks/corpus/README.md` §Measurement Protocol
   and `.claude/rules/benchmarking.md`.
3. **F4 (session continuity, not yet started as of this writing) must not resume the
   checker session.** Phase 1's design constraint — "fresh session, never resumed" —
   is what makes the checker a checker rather than a continuation of the maker's own
   context. `verifier_cli.rs`/`postmortem_cli.rs` never pass `--resume`; a unit test
   in each (`grade_via_cli_argv_never_includes_bare_or_resume`,
   `postmortem_cli_argv_never_includes_bare_or_resume`) pins this. If F4 introduces a
   shared session-continuity helper for worker spawns, it must not be reused for
   these two call sites without deliberately re-deciding this constraint, not by
   accident.
4. **F9 (Evidence and answerability) is now unblocked** — F1's own brief named this as
   the dependency ("An evidence bundle whose verifier verdict is always `true` is
   worse than no bundle"). The verifier verdict is no longer always `true`; F9 can
   proceed on that basis, though a real live confirmation (a `lopi sail` run, no
   `ANTHROPIC_API_KEY`, a planted rubric violation actually rejected) is worth doing
   once more on whatever machine ends up running F9, since this sprint's own live
   confirmation ran in the same sandboxed container as KT-1.3's `--bare` finding.

---

## Next Session — after Sprint F2 (Correctness Holes) — Sprint F3 decouples logs; two inherited items for F1/F5

**Sprint F2 closed four defects (scorer stack detection, the unevaluated-repo
silent-pass, hardcoded pricing, stale model IDs), pinned `--bare` explicitly at
every spawn site, and relabeled the token-pressure gauge rather than replacing it
(no keyless Claude tokenizer exists for the job).** Read first, in order: `CLAUDE.md`,
`CHANGELOG.md`'s `[0.28.0] — Sprint F2` entry, `LEDGER.md`'s two `Sprint F2` entries
(the unevaluated-repo one-way door, and the model-config/`--bare` one-way door), all
four `.konjo/killtests/F2/KT-2.*.md` files, then this file's own words below.

Sprint F3 (log decoupling — named in this sprint's own brief, not otherwise
detailed here) is next. Three things carry forward, not F3's own scope but worth
knowing before touching adjacent code:

1. **The scorer/nextest inconsistency, explicitly deferred to F5.** Phase 1 added
   pytest/Go/Gradle/Maven/pnpm/yarn detection to `crates/lopi-agent/src/
   scorer_detect.rs`, reusing lopi's own CI precedent where it made sense — but
   `.github/workflows/konjo-gate.yml:170-175` runs `cargo-nextest` for the Rust
   path while the scorer still runs `cargo test --quiet`. F2's brief was explicit
   this alignment is F5's job, not F2's; this note is the handoff, not a new
   finding. Whoever picks up F5 should also check whether the newly-added
   pytest/Go/Gradle/Maven paths have their own CI-vs-scorer runner mismatches
   worth aligning in the same pass.
2. **KT-2.4's tokenizer finding — no keyless Claude-accurate tokenizer exists for
   a live, pre-send estimate.** `crates/lopi-context/src/tokens.rs`'s
   `estimate_tokens` still uses `cl100k_base` (OpenAI's GPT-4 BPE), now explicitly
   labeled as an estimate everywhere it's displayed (`TurnMetrics.context_pressure`'s
   doc comment, the web dashboard's "Context pressure (est.)" gauge + tooltip,
   `events.ts`'s turn-metrics log line). This is not fixed, only honestly labeled —
   if Anthropic ever publishes a keyless offline tokenizer for the current Claude
   generation, or lopi's own architecture changes so a post-hoc real count (already
   flowing through `TurnMetrics`/`UsageAccrual` from the CLI's streamed usage) could
   feasibly substitute for the pre-send estimate, that's the trigger to revisit this
   — see `.konjo/killtests/F2/KT-2.4.md` for the full reasoning on why post-hoc real
   data can't serve the pre-send role today.
3. **Phase 6's `--bare` policy, for F1 if F1 has not yet landed by the time you read
   this.** `apply_cli_caps` (`crates/lopi-agent/src/claude_support.rs`) now takes an
   explicit `bare: bool` parameter; all three current spawn sites (all worker
   sessions) pass `false`. **If F1 adds a checker/post-mortem CLI spawn site, it
   should pass `bare: true`** at that new site — per both sprints' coordination
   note, whichever sprint lands second inherits the other's `--bare` decision rather
   than re-deriving it. Check `git log`/`CHANGELOG.md` for whether F1 has landed
   before assuming this note is still live; if F1's own KT-1.3 already found the
   checker needs project context, that live finding wins over this default.

Also worth knowing, not urgent: Phase 4's model-ID config
(`crates/lopi-agent/src/model_config.rs`, `models.toml`) and Phase 3's pricing config
(`crates/lopi-agent/src/pricing.rs`, `pricing.toml`) both read their override file
once per process (cached via `OnceLock`) — a running `lopi sail` process needs a
restart to pick up an edited `.lopi/models.toml`/`.lopi/pricing.toml`, not just a
file save. If a future sprint wants live-reload, that's new scope, not a bug in this
one — the brief only asked for "no recompile," not "no restart."

---

## Next Session — after Sprint F0 (Honesty Pass) — Sprint F1 wires the verifier; two attended actions still owed; one security gap found

**Sprint F0 made every TOON/token performance claim trace to a committed measurement or
deleted it, confirmed `whatsapp` is unreachable (not deleted), corrected several
README/Safety claims that turned out inaccurate on read-through, stated the file-size
gate's scope explicitly, and added an advisory reachability check to G1.** No runtime
behavior changed. Full detail in `CHANGELOG.md`'s `[0.27.1] — Sprint F0` entry and
`LEDGER.md`'s Sprint F0 entry (the WhatsApp positioning call).

**Two things F0 could not complete in an unattended agent session — do these first,
attended, before trusting any pass-rate or benchmark number this repo publishes:**

1. **KT-0.1's live component:** `./benchmarks/run.sh --tasks T01` against a real Claude
   subscription. The dry-run passed (harness not bit-rotted), but a live single-task run
   needs a human watching, per the sprint's own gate definition. Do this, confirm the
   harness still produces a real result end-to-end, before Phase 3.
2. **Phase 3 — run the T01–T10 corpus and commit `benchmarks/results/<ts>_corpus/`.**
   Real money, real wall-clock, ten agent tasks. Follow
   `benchmarks/corpus/README.md` §Measurement Protocol and `.claude/rules/
   benchmarking.md` (≥5 warmup runs, documented hardware, p50/p95/p99, a new
   timestamped directory, never overwrite). Once committed, rename "Expected Pass
   Rate" → "Measured Pass Rate" in `benchmarks/corpus/README.md` and remove the
   "Status (Sprint F0...)" note at the top of that file — state `n=1` explicitly unless
   more than one run was done. If a task the table predicted at 100% fails, record that
   as the finding; do not re-run silently until it passes.

**Sprint F1 (named, not yet started) — wire the verifier/judge/post-mortem.** F0's README
audit confirmed this precisely: `Runner::with_api` (`crates/lopi-agent/src/runner/mod.rs`)
is never called anywhere in the built binary — `grep -rn "with_api" crates/
lopi-orchestrator/` and `src/` both come back empty. That means the verifier
(`runner/verifier_runner.rs`), LLM-judge acceptance (`runner/eval_runner.rs`), and
post-mortem (`runner/run_loop.rs`, gated on `adaptive_retry`) all silently no-op on
every run today, regardless of CLI flags or config — a task can complete "successfully"
with none of them having run. `--stability-gate` is the one layer that does work,
because `src/run_command.rs` wires its *own*, separate `AnthropicClient` outside
`with_api`. F1's job: decide where `with_api` should be wired (the pool? `lopi run`
directly? both?), wire it, and update the README's "no separate API key required"
caveat (currently accurate as of F0, but only because these checkers are confirmed
dead — F1 changes that on purpose).

**Also found during F0's audit, not fixed (out of scope for a measurement/docs sprint,
but real):** `allow_self_modify: false` (`crates/lopi-core/src/config.rs`) — meant to
keep lopi from modifying its own `src/`/`crates/` — is enforced only on the `lopi run`
CLI path and the REPL (`src/run_command.rs`, `src/repl/actions.rs`). The `lopi sail`
web dashboard's task-creation API (`crates/lopi-ui/src/web/handlers.rs::create_task`)
and the `lopi_submit_task` MCP tool (`src/mcp_commands/mod.rs`) never check it — a task
submitted through either of those two paths, pointed at the lopi workspace itself, is
not blocked. This is a real containment gap on two of the four task-entry points the
README itself documents, not a documentation nuance. Whoever picks this up: add the
same check those two paths are missing, or centralize it somewhere all four entry
points share so it can't silently drift again.

**Lower priority, tracked not blocking:**
- `.konjo/scripts/reachability_check.py` (new, advisory) currently flags ~20 `pub mod`s
  as unreferenced elsewhere in the workspace. Most are probably legitimate internal
  surface (tests, future library use); triage the list once, decide which (if any)
  should actually be `pub(crate)` or deleted, rather than leaving it as permanent noise.
- Phase 5 (file-size gate) left `web/`/`macos/` unscoped by design — `web/src/lib/
  stores/stack.ts` (~2,200 lines) would need to be split before the gate could safely
  extend there. Someone should decide whether that split, and the gate extension after
  it, is worth doing.
- `LEDGER.md`'s Sprint F0 entry flags that whether WhatsApp gets *wired up* or
  *deleted* is unresolved — this sprint only established it's currently unreachable.

---

## Next Session — after Sprint S9 (the recipe library) — three confirmed runner gaps, HV-4 portability, deferred CLI

**Sprint S9 shipped `recipes/`** — six documented, live-run-measured `.lopi/loop.toml`
recipes plus the format contract (`recipes/README.md`) and an F0–F10 principle legend
this sprint introduced (no prior canonical numbering existed). `CHANGELOG.md`'s
`[Unreleased] — Sprint S9` entry has the full list; `recipes/README.md` is `decays:
state`, re-verify it before trusting its cost/duration numbers. Four of six recipes
(`fix-failing-test`, `lint-burndown`, `flaky-test-hunter`, `triage-issues`) live-ran to a
clean, measured success. Two (`dependency-bump`, `doc-drift-check`) did not — not because
the recipes are wrong, but because live-running them surfaced real runner bugs, reported
honestly rather than retried into a fake clean number. Three things carry forward:

1. **`src/mcp_commands/mod.rs::submit_task` never applies `RepoProfile`.**
   `run_command.rs:177` and `src/repl/actions.rs:101` both call
   `RepoProfile::load_from_repo(&repo).apply(&mut task)`; the MCP `lopi_submit_task` tool
   builds a bare `Task::new(goal)` and never does. Confirmed live: `dependency-bump` and
   `doc-drift-check` each correctly did the right work three times in a row (a real
   `cargo update`/doc rewrite, verified by hand on the resulting branch) and were
   hard-rolled-back every time by `DiffChecker`'s default `allowed_dirs` (`["src/",
   "tests/"]`) because the `.lopi.toml` `allowed_dirs` override meant to fix that never
   reached the task — reproduced twice, independently, same root cause both times. Fix:
   add the same `RepoProfile::load_from_repo(&repo).apply(&mut task)` call to
   `submit_task` that the other two entry points already have. Real-world impact: any
   task submitted through the Claude Code/Desktop MCP plugin integration against a repo
   with a customized `.lopi.toml` silently ignores it today.
2. **`LoopConfig::permission_allow`/`permission_deny` (the flat, legacy fields) have no
   runtime effect.** Traced both `crates/lopi-orchestrator/src/pool/run_loop.rs` and
   `src/run_command.rs`: both wire `--allowedTools`/`--disallowedTools` purely from
   `LoopConfig::resolved_budget()` (i.e. `[budget]`'s preset deny-list +
   `budget.permission_allow`), never from the flat fields. The flat fields' only live
   code path is `lopi loop show`'s display (`src/loop_commands.rs`). This means the field
   doc comment's claim ("genuinely enforced even alongside
   `--dangerously-skip-permissions`") is stale — true of the mechanism that predated
   `[budget]`, not of what ships today. `recipes/triage-issues/README.md`'s "Known
   containment gap" section is the full write-up; that recipe's safety story currently
   rests entirely on `autonomy_level = "report_only"`, not on `permission_deny`. Fix:
   either wire the flat fields into `resolved_budget()` (extra denies beyond the preset's
   own list) or deprecate them outright so the doc comment stops overclaiming.
3. **`run_guard_command` (the `gate`/`until` shell runner,
   `crates/lopi-core/src/loop_config.rs`) inherits the parent process's stdio.** Harmless
   for the CLI (`lopi run`), where that's just extra visible terminal output. Corrupts
   `lopi mcp-serve`'s stdio JSON-RPC transport when a recipe's `gate`/`until` command
   produces its own stdout (e.g. `dependency-bump`'s `cargo test --workspace` gate) —
   confirmed live, worked around in this sprint's own test harness by tolerating/skipping
   non-JSON lines, not fixed at the source. Fix: redirect the guard command's stdout/
   stderr (`Stdio::null()` or captured, per the existing doc comment's stated intent that
   only the exit code matters).

Also carried forward, unrelated to the three bugs above:

4. **A recipe-authoring gotcha, not a bug**: `Deliverable::infer_from_goal`
   (`crates/lopi-core/src/deliverable.rs`) infers whether a zero-diff attempt is a
   legitimate success from whole-word verbs in the goal text — `write`/`update`/`create`/
   `edit`/etc. imply file changes are expected (zero diff = retry), `summarize`/`review`/
   `analyze`/etc. imply review-only (zero diff = success). `triage-issues`'s goal
   originally said "**write** a one-paragraph summary" and failed three times despite
   explicitly saying "do not create files," because `write` matched the change-verb list
   first. Reworded to "**summarize** the report" and it succeeded in one attempt. Worth a
   lint or at least a callout in `recipes/README.md`'s format contract for anyone adding a
   report-only recipe later.
5. **HV-4 (recipe marketplace/portability)** — explicitly out of scope for S9 ("No
   recipe marketplace, registry, or sharing infrastructure"). The six recipes here are
   the proof-of-concept; HV-4 is packaging/distributing that pattern beyond this repo,
   sized as its own sprint.
6. **`lopi recipes list|show|apply` CLI** — deferred per the sprint brief's own
   instruction ("Only if the copy workflow is demonstrably awkward... Decide this from
   the Phase 2 experience, not in advance"). The copy workflow (`mkdir .lopi && cp
   recipes/<name>/loop.toml .lopi/loop.toml`) proved perfectly workable across all six
   live-run applications in this sprint — no CLI command was needed. Revisit only if a
   future session finds the plain-directory approach genuinely awkward in practice.

---

## Next Session — after Sprint S4 (quality gate enforcement) — coverage climb, rustdoc cleanup, sqlx migration, kiban migration

**Sprint S4 closed Problems 1 and 2 from the quality-gate brief** — a hard,
never-regress coverage floor locked at 68.34% (`.konjo/coverage-floor.txt` +
`.konjo/scripts/coverage_floor_check.py`, kill-tested), a soft-gate convention lint
(`.konjo/scripts/soft_gate_lint.py`, kill-tested) that makes the `KNOWN DEBT, verified
<date>` / `ADVISORY BY DESIGN` comment convention mechanical instead of hand-held, and
made `cargo deny`/`cargo audit` hard gates after triaging every finding (four crate
upgrades, one migrated config schema, four scoped license exceptions/allowances, five
scoped advisory ignores — see `CHANGELOG.md`'s `[0.26.0]` entry for the full list and
`LEDGER.md`'s `Sprint S4` entry for the *why* behind the one-way-door coverage floor,
the lopi-local soft-gate lint, and the `unmaintained`/`unsound` schema migration).
Five things carry forward, not forgotten:

1. **The coverage climb toward 80%, then 95%, is still owed — this sprint locked the
   floor at 68.34%, it did not raise it.** The soft 80%/95% gate (`konjo-gate.yml`,
   "Coverage gate" step) still has its own `KNOWN DEBT, verified 2026-07` comment
   pointing at the lowest-coverage crates as the next step. Every future sprint that
   raises real coverage should also ratchet `.konjo/coverage-floor.txt` up in the same
   PR — the mechanism only protects a number that keeps moving.

2. **The rustdoc intra-doc-link cleanup (`lopi-agent`'s `StreamEvent` x4,
   `lopi-orchestrator`'s `JobScheduler`/`TopologyHint`/types) is still deferred.**
   Explicitly out of scope for S4 (real doc-writing work, not config-scale). The
   "Documentation gate (rustdoc)" step in `konjo-gate.yml` still carries its own
   `KNOWN DEBT` comment with the grep-and-qualify next step.

3. **The sqlx 0.7 -> 0.8+ major-version migration is now the single blocker on four
   RUSTSEC advisories** (RUSTSEC-2026-0098/-0099/-0104 on `rustls-webpki`,
   RUSTSEC-2024-0363 on `sqlx` itself — all pinned via sqlx 0.7.4's own `rustls 0.21`
   dependency; `cargo update -p sqlx`/`-p rustls-webpki` both confirm no
   0.7-compatible fix exists). This is real application-code migration work — sqlx's
   query/macro API changed across majors, and `lopi-memory` is the crate map's whole
   SQLite persistence layer — sized as its own sprint, not a burn-down line item. Both
   `.konjo/deny.toml` and `.cargo/audit.toml` carry matching, reasoned
   `[advisories.ignore]` entries for all four IDs; keep the two lists in sync if either
   changes, and delete both sets of entries in the same PR that lands the sqlx bump.

4. **`.konjo/scripts/coverage_floor_check.py` and `.konjo/scripts/soft_gate_lint.py`
   are lopi-local by necessity, not by choice — pre-flight kill-test 3 confirmed kiban
   `v1.4.0` ships neither mechanism** (`konjo-gates-py` is Python/ML-repo-scoped;
   `konjo-gates-rs`, the Rust equivalent, is an explicit phase-1 stub whose `main.rs`
   only prints `"konjo-gates-rs: phase 1"`). Whoever picks up `konjo-gates-rs`'s real
   implementation should port both scripts' kill-tested behavior
   (`.konjo/scripts/test_coverage_floor_killtest.sh`,
   `.konjo/scripts/test_soft_gate_lint_killtest.sh` — 5 fixture cases each) into the
   crate rather than re-deriving the edge cases from scratch, then retire the
   lopi-local copies in favor of consuming kiban.

5. **`cargo-deny`'s `multiple-versions` duplicate-crate warnings and
   `license-not-encountered` warnings are pre-existing, non-blocking noise** — `cargo
   deny check` reports them but they don't fail the gate (`bans.multiple-versions =
   "warn"`, not `"deny"`). Not touched this sprint (out of scope: no new production
   code paths, config-scale only), but worth a look if a future sprint is already
   auditing the dependency graph — several duplicate versions (`base64`, `hashbrown`,
   `itertools` x3, `windows-sys` family) look collapsible.

---

## Next Session — after Sprint S2′ (egress re-verify + provenance surfacing) — the deferred human gate, and the VPS-tied phases

**Sprint S2′'s kill-test found its own brief's baseline stale — Sprint S2 (below)
had already shipped the deny-by-default Telegram egress allowlist this sprint was
asked to build, so no code changed there. What did land: `GET /api/tasks` and
`GET /api/tasks/:id` now surface a `provenance` field (`"operator"` / `"untrusted"`
/ `"unknown"`), derived from `Task::source` (already persisted, never previously
read back).** Read `docs/security/EGRESS_SURFACE.md` first — the re-verification
kill-test, `decays: state`, re-run it before trusting it — and `LEDGER.md`'s
`Sprint S2′` entry for why a stale brief was treated as a finding rather than a
green light to rebuild. Two things carry forward:

1. **The human gate on egress that this marker is a foundation for is still
   deferred, and still tied to the VPS/webhook path's return.** `notify_loop`
   (`crates/lopi-remote/src/telegram/notify.rs`) doesn't check `provenance()` at
   all today — it only checks `egress_allowed_chat_ids`. When the VPS returns and
   untrusted-origin (`TaskSource::Webhook`) notifications become a live concern
   again, the gate is now cheap to add: reuse `require_plan_approval`'s existing
   shape, but applied to the *send*, not just the *plan* — e.g. hold or
   flag-in-text any `ReportReady`/`TaskCompleted` notification whose task's
   provenance is `"untrusted"`, the same way Sprint S2 Phase 5 already holds
   *execution* on those tasks pending plan approval. Don't build it against a
   loopback-only deployment with no reachable untrusted-webhook path — that was
   this sprint's own reason for stopping at "record and surface."
2. **All four items from the Sprint S2 entry below are unchanged and still
   open** — S3 per-user identity, tool-execution sandboxing, the dormant
   `whatsapp::serve` path's optional-by-default HMAC (still true: confirmed
   again in `EGRESS_SURFACE.md` §1 that no outbound WhatsApp sender exists to
   need the egress allowlist, but its *inbound* signature-verification gap is
   untouched), and `WebConfig.host`'s dead configuration.

---

## Next Session — after Sprint S2 (trifecta containment) — S3 identity, tool sandboxing, dormant paths

**Sprint S2 closed F10's five phases** — auth fail-closed (`--insecure-no-auth`,
refused on non-loopback), CORS allowlist, webhook secret (confirmed already fixed,
no change needed), a deny-by-default egress allowlist for `lopi-remote`'s automated
Telegram sends, and a human-approval gate on any task seeded from `TaskSource::Webhook`
(reusing the pre-existing L2 plan-approval mechanism, not a new one). Read
`docs/security/TRIFECTA_PATHS.md` first — it's the full untrusted-input-to-side-effect
inventory, `decays: state`, re-verify it before trusting any of it — and
`LEDGER.md`'s `Sprint S2` entry for the *why* behind each breaking default and the
two scope decisions (Telegram excluded from the forced-approval gate; the dormant
WhatsApp path still got it). `CHANGELOG.md`'s `[0.25.0]` entry has the full diff
summary. Four things carry forward, not forgotten:

1. **S3 — per-user identity.** Explicitly out of scope for S2 ("no auth provider
   integration... deliberately a separate sprint with a larger blast radius"). S2's
   single shared bearer token is a real improvement over no auth at all, but it's
   still one token for every caller — no per-user attribution, no per-user
   revocation. OAuth/SSO/identity-provider integration is S3's job, sized and staffed
   as its own sprint given the blast radius the brief already named.

2. **Tool execution sandboxing / container isolation.** Also explicitly out of scope
   ("real, valuable, much larger"). The agent runner has full code-exec, git, and PR
   capability with no sandbox boundary — S2's containment is entirely about *when* a
   run is allowed to happen (auth, provenance-gated approval) and *where* its output
   can go (CORS, egress allowlist), not about bounding what a running agent can touch
   on the host. A real fix here is process/container isolation for tool execution,
   scoped as its own sprint.

3. **`crates/lopi-remote/src/whatsapp.rs::serve` is dead code in the built binary** —
   confirmed via `grep -rn "whatsapp::serve" src/ crates/`, matches only its own
   crate's tests. Its inbound `/task` path got S2's `require_plan_approval` gate (so
   that containment travels with it), but its Twilio HMAC verification is still
   optional-by-default with no CLI wrapper enforcing a policy the way
   `webhook_commands.rs` does for the GitHub webhook — because there's no CLI wrapper
   at all. Whoever wires this up next (a `lopi serve-whatsapp` command, presumably)
   should give it the same fail-closed treatment `enforce_webhook_secret_policy` gives
   the GitHub path, not inherit the "accepts unsigned when unset" library default.

4. **`crates/lopi-core/src/config.rs`'s `WebConfig.host` is dead configuration** —
   parsed from `lopi.toml` but never read; `Sail`'s actual bind host only ever comes
   from the CLI `--host` flag. Noted in passing during S2's kill-test, not a security
   issue, just pre-existing drift — either wire it up (so `lopi.toml`'s `[web].host`
   actually does something) or remove it so the config doesn't lie about what's
   configurable.

---

## Next Session — after Doc-Integrity Phase 4 (kiban's `decays:` gate landed) — three deferred items

**Doc-Integrity corrected `docs/LOOP_ENGINEERING_ROADMAP.md`'s state table and
per-sprint status (9/18 sprints already shipped), banner-labeled 12 historical
audit docs, reconciled `FEATURE_STATE_FINAL.md`'s F1–F8 findings (all 7 fixed
at the source level), and — once `konjoai/kiban@v1.4.0` shipped the `decays:`
convention — pulled it in and wired `konjo-doc-staleness` into
`.github/workflows/konjo-gate.yml` as a hard gate (`G0 · Doc Staleness`).**
Read `CHANGELOG.md`'s `Doc-Integrity Phase 4` entry and `LEDGER.md`'s two
`Doc-Integrity` entries first (why the gate clones kiban instead of
`pip install`ing it; why `PLAN.md` was deliberately left unstamped rather than
given a fabricated `verified-against`). Three things remain, not forgotten:

1. **`PLAN.md` is stale — frozen at v0.19.0 (2026-06-18) while `main` is
   `v0.24.0` — and needs `decays: state` front matter once it's caught up.**
   Found while sourcing this sprint's `LEDGER.md` entry, flagged with a
   "known drift" note in `PLAN.md` itself rather than fixed (fixing it means
   re-auditing everything shipped across five versions — out of scope for a
   docs-correction sprint), and deliberately left unstamped rather than given
   a fabricated `verified-against` (stamping it today would repeat the exact
   failure this sprint exists to catch, one file over). The next session that
   touches `PLAN.md` should catch up its "Shipped" log and "Current Health"
   table to `main` the same way this sprint caught up
   `docs/LOOP_ENGINEERING_ROADMAP.md`, then add `decays: state` front matter
   in the same pass so `G0 · Doc Staleness` starts enforcing it immediately.

2. **A real live F1–F8 re-verification is still owed.** This sprint's
   reconciliation (`docs/ops/FEATURE_STATE_RECONCILIATION_2026-07-24.md`) is
   source-level only — it confirms the code changed in the right direction
   and each fix has a test, but it does not re-observe behavior in a running
   `lopi sail` with real auth and real spend the way the original Verify-1
   audit did. F2 in particular has one open caveat: the store-layer wiring
   (`runBarePane`/`launchBareCard`) was confirmed, but the `StackPane.svelte`
   markup wasn't traced to confirm a visible run control actually invokes it.
   Schedule this as its own live-audit sprint, same discipline as `FEATURE_STATE_FINAL.md`
   (`?demo=1` forbidden, no CI sandbox, real subscription auth).

3. **`docs/ui/UI-2-VV-report.md` WARNs on the new doc-staleness gate** ("historical
   doc lacks a dated banner") because this repo's git history can't resolve a
   calendar date for its baseline (`PR #64`, `55338d5`) — left honest rather
   than guessed. If a real date turns up (GitHub's PR#64 merge timestamp,
   checked from outside this repo's local history), add it to the banner and
   the WARN clears.

Also worth a look, not urgent: this sprint's brief scoped an *optional* Phase 5
— a `cargo xtask capability-matrix` (or `lopi state`) command that mechanically
probes the four claims this sprint had to hand-verify (MCP call sites +
`McpServe` registration, `worktree.rs` existence + agent-path wiring,
`lopi-skill`'s `registry.rs`/`invocation.rs`, `VerifierAgent::new`'s isolation
default) and emits the table the roadmap embeds instead of hand-maintains.
Not built this sprint (scope was correction, not new tooling) — building it
would make the next drift mechanical to catch instead of requiring another
manual audit.

---

## Next Session — after Constraint-Capture-2 (mine_patterns finally writes a constraint) — Phase 1 is now unblocked

**Constraint-Capture-2 closed the gap where `mine_patterns` recorded stats but
never a constraint, and gated the newly-populated constraints behind a
promotion threshold. Its own Phase 1 (toolchain-scoped retrieval) was not
attempted during the sprint itself, because the stated dependency (Session
Prompt 1) hadn't landed yet — but it landed on `main` as `Onboarding-Import-1`
(below) while this PR was still open, and had to be merged in.** Read first,
in order: `CLAUDE.md`, `CHANGELOG.md`'s `Constraint-Capture-2` entry (updated
at merge time), `LEDGER.md`'s `Constraint-Capture-2` entry in full (especially
the KT-C/KT-D findings, the promotion-gate numbers, and the merge-time
`PatternExtra`/`upsert_pattern_row`/COALESCE reconciliation), then this file's
own words below and `Onboarding-Import-1`'s entry underneath it.

### Phase 1 (toolchain-scoped retrieval) is now buildable — read this before starting

At sprint time, a full grep of `schema.sql`, `CHANGELOG.md`, and `LEDGER.md`
for `toolchain`/`onboarding`/`detect_stack` found nothing. That has changed:
`Onboarding-Import-1` added `patterns.toolchain` (nullable, populated only by
the onboarding backfill path — still `NULL` for every live-mined
`mine_patterns` row) and `patterns.source` (`'lopi_run'` vs.
`'onboarding_import'`). **Before starting Phase 1, re-confirm the current
state rather than trusting this note indefinitely** — schemas can keep moving
between sessions, the same lesson this merge itself is proof of. Two real
open questions Phase 1 will hit immediately:
- `find_similar_patterns` has no toolchain parameter yet — adding one is
  additive (default to unscoped, matching this sprint's Constraint-Capture-2
  brief's own "backward-compatible: an unscoped call still works" framing).
- Live-mined patterns (`mine_patterns`, the path this sprint's own constraint
  capture feeds) still don't populate `toolchain` at all — only the
  onboarding backfill path does. Toolchain-scoping a *live* task's retrieval
  would need `mine_patterns` to also call `src/toolchain_detect.rs` (or
  receive a toolchain hint from its caller), which today it does not. Decide
  whether Phase 1 should close that gap too, or scope toolchain retrieval to
  backfilled patterns only for a first cut.

### What shipped this sprint

- `MemoryStore::mine_patterns` gained a `success_constraint: Option<&str>`
  parameter, writing it into `patterns.successful_constraints` on both insert
  and update. All three real call sites (`pool/run_loop.rs::run_one`,
  `src/run_command.rs::run_with_live_print`, `src/repl/actions.rs`) now pass a
  real constraint on a clean success (`matches!(outcome, TaskStatus::Success
  { .. })`), `None` otherwise.
- `patterns.occurrence_count` — new column, incremented on every
  `mine_patterns` update.
- `AgentRunner::success_constraint()` (`crates/lopi-agent/src/runner/
  capture.rs`, new) — derives a bounded constraint from `last_plan`, reusing
  `reflection::summarize_attempt` rather than duplicating it.
- `seed_from_patterns`'s promotion gate (`crates/lopi-agent/src/runner/
  seed.rs::is_promotable`): `occurrence_count ≥ 2` and `success_rate ≥ 0.5`
  for mined patterns; postmortem-derived patterns exempt from both. See
  `LEDGER.md` for the full reasoning and the "how to apply" note on retuning
  these numbers later.
- Phase 1 (toolchain-scoped retrieval) — **not attempted this sprint**, but
  its precondition (Session Prompt 1 / `Onboarding-Import-1`) landed at merge
  time — see above for what's now unblocked and what's still missing.
- New tests across `crates/lopi-memory/src/store/tests.rs`,
  `crates/lopi-agent/src/runner/capture.rs`, and
  `crates/lopi-agent/src/runner/seed.rs`, including a live-verification test
  (`live_check_backfilled_pattern_constraint_reaches_the_real_planning_prompt`)
  that drives a file-backed store through the real `mine_patterns` →
  `gather_seed` → `claude_support::build_plan_prompt` pipeline and asserts the
  backfilled constraint appears in the literal planning-prompt text.
- `cargo build --workspace`, `cargo test --workspace` (all crates), `cargo
  clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, and
  `RUSTDOCFLAGS="-D missing_docs" cargo doc` all clean. `VERSION` (workspace
  `Cargo.toml`) bumped to `0.24.0` at merge time — this sprint and
  `Onboarding-Import-1` (below) both independently bumped to `0.23.0`, but
  that version had already shipped on `main` without this sprint's changes
  by the time the two were reconciled, so this sprint's own bump moved to
  `0.24.0` rather than silently reusing an already-released version number.

### What could not be verified in this sandbox — needs a live check

**No live Anthropic API session exists in this sandbox to run `claude -p`
itself**, the same standing constraint recorded in every prior sprint's
`LEDGER.md` entry (Sprint Successor-1, MCPB-App-1/2). What was verified
instead: a real, file-backed `MemoryStore`, backfilled through the real
production write path, feeding the real `gather_seed()` → real
`build_plan_prompt()` — the literal string `ClaudeCode` hands to the `claude`
CLI subprocess for both its one-shot and streaming plan paths, confirmed (via
`--nocapture`) to contain the backfilled constraint. A session with real
`claude -p` access should, once Phase 4 (below) or any other planning-prompt
change lands: submit a real task in a repo with backfilled pattern history,
confirm the printed/logged planning prompt (or a debug log of it) contains a
non-empty constraint sourced from a prior pattern, and confirm the resulting
implementation actually reflects it (not just that the string was present in
the prompt).

### Open items for a future sprint

- **Phase 4 (stretch, explicitly deferred by this sprint's own brief) — a
  promoted pattern as a live composer suggestion.** Not started. The brief's
  own kill-test for this phase (does `web/src/lib/components/Composer.svelte`
  have a hook point for this without disrupting the `;`-prefix verb grammar
  work) was never run. Scope as its own sprint if picked up.
- **Toolchain scoping (Phase 1)** — no longer blocked on schema; see the new
  section above for what's ready and what still needs deciding (live-mined
  patterns don't carry a toolchain today, only backfilled ones do).
- **The constraint-update policy is COALESCE (never overwrite once set), not
  this sprint's originally-designed overwrite-latest** — reconciled at merge
  time against `Onboarding-Import-1`'s already-shipped `upsert_pattern_row`
  semantics (see `LEDGER.md`). Revisit only with real evidence a stale
  constraint is stuck wrong on a row, not on intuition.

## Next Session — after Onboarding-Import-1 (Toolchain-Scoped Pattern Backfill)

**Onboarding-Import-1 shipped all five phases and both hard exit-gate
checks that don't require Wes's real machine — KT-A/KT-B (live,
`~/.claude`-access-dependent kill-tests) are correctly left open, not
assumed.** Read first, in order: `CLAUDE.md`, `CHANGELOG.md`'s
Onboarding-Import-1 entry, `LEDGER.md`'s Onboarding-Import-1 entry in full
(the KT-C naming confirmation and the honest KT-A/KT-B scope limits), then
this file's own words below.

### What shipped this sprint

- **Schema (Phase 0):** `patterns.toolchain` (nullable), `patterns.source`
  (`DEFAULT 'lopi_run'`), and a new `onboarding_imports` idempotency ledger
  table — all backward-compatible `ALTER TABLE`/`CREATE TABLE IF NOT EXISTS`.
- **Transcript reader (Phase 1):** `crates/lopi-agent/src/transcript_import.rs`
  — defensive NDJSON decoder for `~/.claude/projects/**/*.jsonl`, built
  against a real captured line from this very session's own transcript.
- **Toolchain detection (Phase 2):** `src/toolchain_detect.rs` — manifest-file
  based (`Cargo.toml`→rust, `package.json`→node, `pyproject.toml`/
  `requirements.txt`→python, `go.mod`→go, `Gemfile`→ruby), the first toolchain
  detection anywhere in lopi.
- **Backfill store path (Phase 3):** `MemoryStore::backfill_onboarding_pattern`
  reuses a shared `upsert_pattern_row` helper with `mine_patterns` — one write
  path, not two, per the brief's own hard reuse constraint.
- **Constraint extraction (Phase 4):** `session_looks_successful()` +
  `extract_success_constraint()` — a clean tool-result tail *and* explicit
  success language in the final assistant text, both required, documented
  in-code as a deliberately conservative heuristic (false positives here
  pollute `successful_constraints` with bad guidance).
- **CLI + idempotency (Phase 5):** `lopi import [--dry-run] [--claude-dir]`,
  idempotent on the transcript's own `sessionId`.
- 33 new tests, 1620 workspace tests green, clippy clean, `-D missing_docs`
  clean. Dry-run **and** a real (non-dry-run) round trip both verified against
  this sandbox's one real transcript — actual pasted output below, not a
  description of expected output:

```
$ lopi import --dry-run
🧭 lopi import — scanning /root/.claude (1 transcript file found)
  [dry-run] would import 2afe0e65 · rust · "# Session Prompt 1 — Onboarding Import: Toolchain-Scoped Pat…"

🧭 would import 1 · 0 already imported · 0 with no human turn · 0 with an empty keyword fingerprint

$ lopi import
🧭 lopi import — scanning /root/.claude (1 transcript file found)
  ✅ 2afe0e65 · rust · "# Session Prompt 1 — Onboarding Import: Toolchain-Scoped Pat…" → pattern a711bd4e

🧭 imported 1 · 0 already imported · 0 with no human turn · 0 with an empty keyword fingerprint

$ lopi learn list
🧠 lopi learn — 1 pattern(s)

  Id        Keywords                                   Avg Att.   Success%  Source
  ──────────────────────────────────────────────────────────────────────────────────────────
  a711bd4e  access across actual actually adds agai…        1.0         0%  📊 mined

$ lopi import --dry-run   # re-run: idempotency check
🧭 lopi import — scanning /root/.claude (1 transcript file found)

🧭 would import 0 · 1 already imported · 0 with no human turn · 0 with an empty keyword fingerprint
```

(The 0% success rate is correct, not a bug: this session is still
in-progress, so `session_looks_successful()` rightly found no completion
signal in it yet — the heuristic didn't spuriously mark an unfinished session
as a clean success.)

### What could NOT be verified this session — needs Wes's real machine

**KT-A (needs a real, multi-project `~/.claude/projects` corpus).** This
sandbox's `~/.claude/projects/` holds exactly one file — this session's own
in-progress transcript — not the 3+ files across separate projects
(lopi/squish/kiban) the original brief asked for. The one file available was
still genuinely useful: it directly confirmed the human-turn-vs-tool-result
schema ambiguity (see `LEDGER.md`) from real data, not a guess. What it
*can't* confirm: whether every real historical session across a full corpus
follows the same shape with zero exceptions, and whether any transcript ever
carries a `type: "summary"` entry (a possible richer goal source the brief
raised — none appeared here, so `transcript_import.rs` doesn't special-case
it). A session with real `~/.claude` access needs to: capture 3+ real files
across genuinely different projects, diff them against
`transcript_import.rs`'s assumptions, and either confirm no `summary` type
exists or add a case for it if one does.

**KT-B (needs a real `~/.claude/settings.json`).** This container has no such
file at all (only `launcher-settings.json`, a different SDK-hook config, not
user retention prefs) — `cleanupPeriodDays` is simply unknown here, not
assumed to be the 30-day default or anything else. A session with real access
needs to check it and report how many days of real history exist — if
retention is near the default, that's a real finding about how much signal a
first `lopi import` run actually recovers, worth surfacing to Wes plainly.

### Open questions for a follow-on sprint

- **Continual recognition (this sprint's explicit non-goal, by design).** The
  `toolchain`/`source` schema groundwork this sprint laid is meant to be kept
  populated going forward by a live-capture sprint, not just this one-time
  backfill — that sprint still needs to be scoped and built.
- **Embedding-based clustering** is out of scope unless a future kill-test
  against a real multi-project corpus shows Jaccard/`keyword_fingerprint` is
  inadequate at real scale — untested here, since this sandbox never had more
  than one transcript to test scale against.
- **claude.ai chat export ingestion** (manual, email-delivered ZIP, no live
  API) — flagged as a possible future manual-import command, correctly not
  built this sprint.

---

## Next Session — after MCPB-App-2 (Click Interactivity + Backend Write Path)

**MCPB-App-2 wired the stack-status widget's first click-driven write path —
a Cancel button per row, calling the already-existing `lopi_cancel_task` MCP
tool — but its own Phase 3 (live verification) is explicitly blocked, not
skipped: KT-B3, the widget's basic live render in a real Claude Desktop, still
has not been confirmed as of the most recent `KT-B3-Live` entries.** Read
first, in order: `CLAUDE.md`, `CHANGELOG.md`'s `MCPB-App-2` entry, `LEDGER.md`'s
`MCPB-App-2` entry in full (the KT-1–KT-4 findings and the four "how to apply"
points), then this file's own words below.

### What shipped this sprint (Phases 0–2, all completable without KT-B3)

- Pre-flight kill-tests KT-1 (tool-call symmetry — confirmed, no origin
  branching in `crates/lopi-mcp/src/server.rs`), KT-2 (`callServerTool()` vs.
  `ontoolresult` — confirmed distinct, cancel result wired through the
  former), KT-4 (no autonomy/plan-approval gate on `cancel`/`delete_task` —
  confirmed absent by reading the pool/store code directly). KT-3 (host-level
  approval UX) is unanswerable without a real host — correctly left open.
- `src/mcp_ui/stack_status.html`: a Cancel button on every `queued`/`running`
  row (`isCancelable()`), a confirm-then-two-click-fallback guard
  (`requestCancel`/`doCancel`), real-`disabled`-button double-submit
  prevention, inline `.row-error` on failure, row replaced with a grayed
  "cancelled" line on success, and an `app.updateModelContext(...)` call
  after a successful cancel. `.row` changed from `<button>` to a
  `role="button"` div (nesting a real button inside it was invalid HTML —
  see `LEDGER.md`) with a new `root.onkeydown` restoring keyboard activation.
- `src/mcp_commands/server_wire_tests.rs` — new. Two tests drive
  `lopi_cancel_task` through the real `lopi_mcp::serve()` JSON-RPC loop with
  the real `LopiToolHandler`, not a mock — the surface the brief asked for,
  relocated from the (inaccessible) `crates/lopi-mcp` location the brief
  named, since that crate has no dependency on lopi's actual tool
  implementations by design. `mod_tests.rs`'s `test_state()` is now
  `pub(super)` so both test modules share it.
- 1576 workspace tests green, `cargo clippy --workspace --all-targets -- -D
  warnings` clean, widget's script body still `node --check` clean, `VERSION`
  bumped to `0.22.0`.

### What could NOT be verified this session — needs a live Claude Desktop, and needs KT-B3 first

**Phase 3 did not run at all.** Nothing in this sprint's own testing exercises
whether the widget actually renders in a real host in the first place — that
question (KT-B3) predates this sprint and is still open per `KT-B3-Live`'s
most recent entries (server spawns, MIME type and extension-negotiation fixes
landed, but the widget-render check itself was never observed against a real
Claude Desktop in any session so far). A session with real Claude Desktop
access needs to, **in this order**:

1. Confirm KT-B3 itself first — install the current `.mcpb`, submit a task,
   confirm the stack-status panel actually renders inline (not a text
   fallback / warning toast). If this still fails, that is this sprint's
   blocker, not this sprint's own code — stop and diagnose against
   `KT-B3-Live`'s three prior findings before touching anything here.
2. Only once KT-B3 passes: click Cancel on a real running/queued task,
   observe whether `window.confirm()` fires a native dialog or throws (this
   resolves KT-3 and the confirm-vs-two-click fallback split at once — if
   `confirm()` works, the two-click fallback code path can be considered
   dead code and reconsidered), confirm the task is actually cancelled and
   deleted (`lopi_list_tasks` or a direct DB check), confirm the row updates
   without a full widget refresh, confirm no console errors.
3. Click Cancel on a task that completes between page-load and click —
   confirm the resulting "not found" `error` payload renders as this
   sprint's inline `.row-error`, not a crash.
4. Rapid double-click on Cancel — confirm the real `disabled` attribute
   actually prevents a second `callServerTool()` call (not just a UI-level
   debounce).
5. Capture a screenshot or recording of at least one successful cancel round
   trip as evidence, per this repo's own precedent for live-host checks —
   not just a text claim that it worked.

### Open question carried forward

Whether a real MCP Apps host adds its own approval modal on top of a
widget-initiated `tools/call` (KT-3) is still genuinely unknown — the
widget's own confirm step does not assume either answer, and should not be
simplified away even if a host turns out to add a modal of its own; two
prompts (one host-level, one app-level) for a destructive action is not a
bug.

---

## Next Session — after Sprint Successor-1 (Task Lineage and Containment)

**Sprint Successor-1 built the data model, lineage fields, and containment
gates for agent-authored successor tasks — no agent authoring yet.** Read
first, in order: `CLAUDE.md`, `CHANGELOG.md`'s `[0.21.0]` entry, `LEDGER.md`'s
`Sprint Successor-1` entry in full (the three one-way-door decisions:
`SelfAuthored` vs. `SelfModify`, the autonomy-ceiling clamp, and the
untrusted-source ratchet), then this file's own words below.

### What shipped this sprint

- `lopi-core::successor` — the `Successor` proposal type (`goal`/`when`/
  `rationale`/`allowed_dirs`), `SuccessorCondition`, and `Successor::validate()`.
- `Task` gained `parent_task`, `chain_depth`, `successor_enabled`,
  `successor_fixture` (all `#[serde(default)]`); `TaskSource::SelfAuthored`.
  `TaskSource` moved to its own `task_source.rs` (file-size gate).
- `derive_successor_task(parent, successor, max_depth)` — the four
  containment gates (depth cap, autonomy ceiling, directory inheritance,
  untrusted-source lockdown), each with its own dedicated test.
- `AgentEvent::TaskCompleted` gained `successor: Option<TaskId>`.
- `lopi-memory`: `tasks.parent_task`/`tasks.chain_depth` columns +
  `MemoryStore::lineage_chain` (bounded ancestor walk, not a recursive tree).
- `AgentRunner::derive_and_stash_successor` (finalize.rs, beside
  `emit_report`) + pool-level enqueue via the real `AgentPool::submit` —
  gated on `Task::successor_enabled`, fed by `Task::successor_fixture` only
  (no parsing from agent output — that's this sprint's own hard boundary).
- Pre-flight kill-tests KT-A/B/C all recorded (see `LEDGER.md`); 1574
  workspace tests green, clippy clean.

### What could NOT be verified in this sandbox — needs a live check

**The Phase 4 integration test does not drive a real `claude -p` subprocess
through `AgentRunner::run()`'s full plan → implement → test → score loop.**
That requires a live Anthropic API session, which this sandbox cannot reach
(no `claude` CLI session/network for that path). What was actually verified
instead, and why it's still meaningful:
- `crates/lopi-agent/src/runner/finalize.rs`'s `derive_and_stash_successor_*`
  tests prove a passing `finalize()` call really does invoke
  `derive_successor_task` and stash a gated child — the *logic* seam.
- `crates/lopi-orchestrator/tests/successor_enqueue.rs` proves the derived
  child really does land in the real `TaskQueue` via the real
  `AgentPool::submit` (dedup/topology/audit intact) with lineage/depth/gates
  correct on the popped task — the *plumbing* seam.
- **Not yet verified: that a real end-to-end task run (real git repo, real
  `claude -p` session, real diff, real commit) that reaches `TaskStatus::
  Success` actually produces a `TaskCompleted` event with a populated
  `successor` field and a second row appearing in a live `lopi sail`
  dashboard.** This needs a session with real Claude Code CLI access: submit
  a task with `successor_enabled: true` and a `successor_fixture` set (no
  API surface exists yet to set these from the CLI/REST layer — that's
  itself an open question below, KT-1) against a real repo, watch it run to
  completion, and confirm the successor task appears queued and eventually
  dispatched.

### Open questions for Sprint Successor-2

- **KT-1 — no submission surface for `successor_enabled`/`successor_fixture`
  exists yet.** Neither `lopi run`'s CLI flags, the REST `POST /api/tasks`
  handler, nor `.lopi/loop.toml` expose a way to set these fields today —
  this sprint only exercises them via directly-constructed `Task` values in
  tests. Before Sprint Successor-2 adds parsing-from-agent-output, decide
  where a human-supplied fixture successor should be configurable from (a
  repo-level `.lopi/loop.toml` default? a per-task REST field? both?) —
  otherwise the only way to use this sprint's plumbing today is a hand-built
  `Task`.
- **KT-2 — `DEFAULT_MAX_CHAIN_DEPTH = 3` is a hardcoded constant, not a
  per-repo config.** `crates/lopi-core/src/successor.rs` documents this as a
  deliberate scope cut (a natural `.lopi/loop.toml` ceiling once chains
  actually run unattended for a while), but it means every repo currently
  gets the same depth cap regardless of how much it trusts self-extending
  chains. Worth revisiting once Sprint Successor-2/3 make chains something
  that actually runs unattended rather than fixture-only.
- Sprint Successor-2's own explicit scope (per the brief that ran this
  sprint): parse a `Successor` out of an agent's own `final_text`, replacing
  the `successor_fixture` config-only path. Sprint Successor-3: branch
  `advance_to_next_step`'s static-goal chain scheduling on top of dynamic
  successors, plus web/macOS lineage rendering. Sprint Successor-4 (gated on
  a hardware kill-test not yet run): `claude --resume`/`StreamEvent::
  session_id()` — explicitly out of scope until then.

---

## Next Session — after KT-B3-Live

**The attended runbook (`LOPI_KTB3_ATTENDED_RUNBOOK.md`) ran for real for the
first time and did not reach the widget-render question — the server failed
to spawn.** Two independent packaging bugs found and fixed this session, both
verified in one green run. Full detail in `LEDGER.md`'s `KT-B3-Live` entry;
short version below.

Read first, in order: `CLAUDE.md`, `CHANGELOG.md`'s `KT-B3-Live` entry,
`LEDGER.md`'s `KT-B3-Live` entry in full, then `LOPI_KTB3_ATTENDED_RUNBOOK.md`
itself.

### What this session found and fixed

1. **`mcpb/manifest.json` used `${platform}`, which is not a real MCPB
   substitution token** — Claude Desktop's MCP log showed it passed through
   literally, so `entry_point`/`mcp_config.command` resolved to a directory
   that never existed and the server hit "Failed to spawn process: No such
   file or directory" before tool discovery could even start. Fixed by
   hardcoding the literal `server/darwin-arm64/lopi` path (the repo is
   `darwin`-only per `compatibility.platforms`, so no `platform_overrides`
   mechanism was needed).
2. **This branch's `mcpb-release.yml` had regressed to `timeout 10`**
   (unavailable on macOS runners) — a `main`-merge timing gap, unrelated to
   Finding 1. Re-applied `perl -e 'alarm 10; exec @ARGV'` directly.
3. Both verified together in run `29770853385` (headSha `467abb8`), smoke-test
   included — green end to end, real `initialize`/`serverInfo` round trip.

### What a session with real Claude Desktop access needs to do next

1. **Discard the stale `.mcpb` in the repo root** (`lopi-bfe4d7bb...`, the
   artifact from the *failed* attempt) and pull the fresh one:
   `lopi-467abb86e6e3408e73fefc7367db9e72d428587c-darwin-arm64.mcpb` from run
   [`29770853385`](https://github.com/konjoai/lopi/actions/runs/29770853385).
2. **Re-run `LOPI_KTB3_ATTENDED_RUNBOOK.md` from step 1.** This time the
   server should actually spawn — confirm that first (no repeat of the
   `${platform}` failure), then continue: tool list (all eight, including
   `lopi_get_stack_status`), submit/check a real task, watch for an actual
   rendered panel vs. silent text fallback.
3. **Given two packaging bugs slipped past the earlier `mcpb pack`/`unpack`
   verification, don't trust that check alone again** — it exercises the
   bundle mechanics, not the manifest's own command-resolution path a real
   host uses. If this session finds a third packaging issue, that's a sign
   the smoke-test step itself needs to install via a real (or real-ish) host
   path, not just unpack-and-invoke.
4. Write the `LEDGER.md` KT-B3 outcome entry per the runbook's own "either
   way" section — this will be the first time that section has real data to
   report instead of "not attempted."

---

## Next Session — after MCPB-App-1

**The next step is the attended `LOPI_KTB3_ATTENDED_RUNBOOK.md` runbook —
not more Claude Code work.** Everything automatable in `LOPI_DISTRIBUTION_
PLAN.md`'s Track B is now built and packaged. Nothing about the actual
render has been verified — that's not an oversight, it's the correct
boundary a sandbox can't cross, per the runbook itself and the `MCP-Serve-1`
KT2 / `MCP-App-1` KT-D2 precedent for this exact class of blocker.

Read first, in order: `CLAUDE.md`, `CHANGELOG.md`'s `[0.19.0]` entry,
`LEDGER.md`'s `MCPB-App-1` entry in full (the branch-persistence decision,
the join fixture, and — importantly — the macOS-build toolchain finding are
all there), this file's own words below, then `LOPI_KTB3_ATTENDED_RUNBOOK.md`.

### What shipped this sprint (all four deliverables, none render-verified)

1. **Branch persistence** — `tasks.branch`, written by `AgentRunner::
   persist_branch` the moment `TaskStarted` fires. Real column, real store
   call, tested.
2. **`lopi_get_stack_status`** — the eighth MCP tool. Joins the roster with
   per-task DAG stage and branch. Verified against a real two-task,
   two-stage concurrent fixture (KT-B2) — real field values, not just
   success/failure.
3. **The `ui://lopi/stack-status` widget** — `src/mcp_ui/stack_status.html`,
   implements exactly the three lifecycle methods specified
   (`ui/initialize`/`ui/notifications/initialized`/`ui/notifications/
   tool-result`), read-only, no interactivity. Bound via `_meta.ui.
   resourceUri`. `lopi-mcp` gained real `resources/list`/`resources/read`
   support to serve it, plus `structuredContent` on every tool call.
4. **`mcpb/manifest.json` + `.github/workflows/mcpb-release.yml`** —
   `mcpb validate`/`pack`/`unpack` mechanics verified for real (caught and
   fixed two schema errors in the process). **The actual macOS arm64
   binary does not exist yet** — see below.

### A real, concretely-checked blocker this sprint found: no macOS arm64
### binary was produced, and this sandbox structurally cannot produce one

This is new — MCP-App-1 and the plan doc both assumed Deliverable 4 was
sandbox-safe ("nothing here needs nested-spawn access or a GUI host"). That
assumption held for KT-B1/KT-B2 and doesn't hold for a real target binary.
Checked two ways, not assumed (full detail in `LEDGER.md`):

1. Plain `cargo build --target aarch64-apple-darwin` fails immediately —
   this sandbox's `cc` is Linux GCC/Clang, incompatible with `ring`'s
   macOS-targeted build flags.
2. `cargo-zigbuild` gets substantially further (past `ring`, past
   `openssl-sys` with vendored OpenSSL) but hits a hard wall at
   `libgit2-sys`'s own `build.rs`, which unconditionally requires Apple's
   Security.framework/CoreFoundation.framework for any `apple` target —
   no feature flag exists upstream to avoid this. Proprietary Apple
   frameworks aren't obtainable in this sandbox, legitimately or otherwise.

**What a session with real macOS access (attended, or a GitHub Actions run
on the new `macos-14` workflow) needs to do:**

1. Trigger `.github/workflows/mcpb-release.yml` for real (currently
   `workflow_dispatch`-only, deliberately not wired to run automatically
   before its first real run is watched end to end) — or run
   `cargo build --release --target aarch64-apple-darwin --bin lopi` plus
   `mcpb pack mcpb` natively on real Apple Silicon hardware.
2. Confirm the resulting `.mcpb`'s binary actually launches `mcp-serve`
   when invoked exactly as `mcp_config` specifies — the workflow's own
   smoke-test step does this already; if run by hand, replicate it (drive
   a real `initialize` over stdio, confirm `serverInfo` comes back).
3. **Then, and only then**, run `LOPI_KTB3_ATTENDED_RUNBOOK.md` against
   that real bundle: install in Claude Desktop, confirm the tools list
   shows all eight tools including `lopi_get_stack_status`, submit a
   trivial task, and watch whether an actual rendered panel appears versus
   silent text-only fallback. Write the `LEDGER.md` entry for whichever
   outcome happens — both are legitimate, complete results per the
   runbook's own framing.

### Explicitly not started, correctly

Phase B2's remaining items (privacy policy doc, README quick-install
section, desktop-extension form submission) all wait behind KT-B3 clearing,
per the plan's own phasing — not attempted here. One consequence worth
knowing before it surprises anyone: `mcpb/manifest.json`'s
`privacy_policies` array points at `PRIVACY.md`, which doesn't exist in the
repo yet — a 404 until Phase B2 writes it. Sideloading (this sprint's whole
distribution path) doesn't require the file to exist for install to work,
only directory listing does, so this doesn't block anything here — just
don't be surprised the link is dead if you follow it now.

### A repo-doc drift worth fixing — flagged a third time now

`LOPI_DISTRIBUTION_PLAN.md`'s repo copy is still the pre-Track-D-merge
draft (no Deliverables 1–2, no KT-B1/B2/B3, no widget mention in its Track
B section). This sprint, like `MCP-App-1` before it, worked from a pasted
up-to-date version rather than the repo's own stale copy. Third time this
exact drift has been logged (`LEDGER.md`'s `MCP-App-1` and `MCPB-App-1`
entries both flag it) — genuinely overdue for a sync pass; a session that
trusts the repo's own file over a pasted one will miss the entire Track
B/D merge.

---

## Next Session — after MCP-App-1

Read first, in order: `CLAUDE.md`, `CHANGELOG.md`'s `[0.18.0]` entry,
`LEDGER.md`'s `MCP-App-1` entry in full, `LOPI_DISTRIBUTION_PLAN.md`'s
Track D section — **but read it with caution, see the drift note below.**

### What happened this sprint — KT-D2 blocked, correctly, nothing shipped

MCP-App-1 attempted Track D (Loop Stacks inline MCP App dashboard). Its own
hard gate, KT-D2 ("does the MCP Apps `ui/initialize` handshake actually
complete in a real Claude Desktop install and a real claude.ai account"),
cannot be run in this sandboxed environment: headless Linux container, no
`DISPLAY`, no macOS/Windows, no authenticated claude.ai session anywhere on
disk. This was checked concretely (`uname`, `$DISPLAY`, `/Applications`,
credential paths, `ps aux` for any usable interactive `claude` session — see
`LEDGER.md` for the exact commands and output), not assumed. Per the sprint
brief's own instructions, that's a legitimate stop: no widget code, no
`ui://` resource, no new tool implementation were written. This is the
correct outcome, not an incomplete one — don't treat "no widget shipped" as
a task left undone.

**What *was* answered, since it doesn't need live hosts:** KT-D3 (the
tool-binding decision). Full reasoning in `LEDGER.md`, short version: the
widget needs a **new aggregating tool**, not a rebind of
`lopi_get_agent_dag`. Neither existing tool covers Deliverable 4's fields
(task roster + branch + live stage-level `TaskStatus`) — `tasks.status` is
coarse (`"running"` for the entire execution, no stage detail), stage
detail only lives in `agent_dag_nodes`, and **branch has no structured
durable source at all** (only an in-memory event, a freeform log line, or
the terminal `Success{branch}` variant). That last point is a new
prerequisite MCP-App-1 found mid-research, not something the original plan
anticipated: **persisting branch as a real column (or dedicated store call)
when `TaskStarted` fires needs to happen before the aggregating tool can be
built cleanly.**

### What a session with real Claude Desktop and claude.ai access needs to check first

1. **KT-D2 itself.** Build the trivial "hello from lopi" `ui://` resource
   exactly as the original brief specified (a static HTML page, bound to
   any throwaway tool), and attempt the real round trip in a real Claude
   Desktop install and a real claude.ai account. If it renders cleanly,
   proceed to KT-D1. If the handshake fails silently (tool call succeeds,
   resource fetch succeeds, no iframe appears), log the exact protocol
   version / SDK version / host version / failure point and treat Track D
   as blocked pending an upstream fix — there's no client-side workaround
   for a host not completing its half of the handshake.
2. **KT-D1**, once KT-D2 clears: with the trivial resource attached,
   confirm a plain-text MCP-Serve-1 tool (not bound to the resource) still
   renders clean text in Claude Code, nothing broken by the resource's mere
   presence elsewhere in the server.
3. **The branch-persistence prerequisite this sprint found**, before
   building the new aggregating tool: decide how branch gets persisted
   structurally (new `tasks` column vs. a dedicated store call keyed on
   `TaskStarted`) — see `LEDGER.md`'s `MCP-App-1` entry for the exact
   places branch currently does and doesn't appear.
4. Only then: Phase D1 (minimal widget against the new tool), D2 (real
   `structuredContent`), D3 (cross-host verify: Desktop, claude.ai, Cowork
   if reachable; confirm Claude Code still degrades cleanly).

### A repo-doc drift worth fixing before it trips up a future session

`LOPI_DISTRIBUTION_PLAN.md` in the repo is stale — it's the pre-`MCP-Serve-1`
draft (Track A still shown as unbuilt, no Track D section at all). This
sprint's brief pasted an up-to-date version (Track A marked shipped, Track D
added) directly into the session rather than pointing at the repo file,
which is the only reason this sprint had the real Track D spec to work
from. Not this sprint's job to fix (same call as the two-`NEXT_SESSION_
PROMPT.md`-files drift already flagged in the MCP-Serve-1 entry below), but
worth a sync pass — a session that trusts the repo's own copy over a pasted
one will miss Track D's existence entirely.

---

## Next Session — after MCP-Serve-1

Read first, in order: `CLAUDE.md`, `CHANGELOG.md`'s `[0.17.0]` entry,
`LEDGER.md`'s `MCP-Serve-1` entry, `LOPI_DISTRIBUTION_PLAN.md` Track A in full.
Confirm `Cargo.toml`'s version matches `CHANGELOG.md`'s top entry before doing
anything else.

### What landed (MCP-Serve-1, all deliverables 1–5 shipped)

1. `lopi mcp-serve` subcommand (`src/mcp_commands.rs`) — the curated seven-tool
   set from the plan's Track A 1.1 table, over stdio, reusing
   `lopi_mcp::server::serve()` unmodified.
2. `plugin/skills/lopi-cli/SKILL.md` (see the layout note below for why it's
   under `plugin/`, not repo-root `skills/`). Documents the CLI as it ships
   today.
3. `plugin/.claude-plugin/plugin.json` + `.claude-plugin/marketplace.json`
   (repo root) + `plugin/.mcp.json`.
4. Local install verified live: `claude plugin marketplace add`, `claude plugin
   install`, `claude plugin details`, and a real `lopi_submit_task` →
   `lopi_get_task` round-trip through the actual installed/cached binary (not
   just the dev build). `claude plugin validate --strict` clean.
5. Stretch goal (submit to `anthropics/claude-plugins-community`, announce
   publicly) — **not done**, correctly optional per the sprint brief, not
   attempted so as not to rush Phase 2/3.

### Layout deviation from the plan — read before touching plugin files

The plan's package layout puts `.claude-plugin/`, `.mcp.json`, and `skills/` at
the repo root. That fails `claude plugin validate --strict` live: it flags this
repo's own root `CLAUDE.md` as invalid "plugin root" content, and `CLAUDE.md` is
real contributor-facing content that shouldn't move or disappear to satisfy a
plugin validator. The actual layout shipped:

```
lopi/
├── .claude-plugin/marketplace.json    # fixed discovery location — stays here
├── plugin/
│   ├── .claude-plugin/plugin.json     # name: "lopi" — immutable, see LEDGER.md
│   ├── .mcp.json                       # ${CLAUDE_PLUGIN_ROOT}/bin/lopi mcp-serve
│   ├── bin/                            # gitignored — built by scripts/build-plugin-bin.sh
│   └── skills/lopi-cli/SKILL.md
├── scripts/build-plugin-bin.sh
└── src/mcp_commands.rs
```

`marketplace.json`'s one plugin entry has `"source": "./plugin"`. If a future
session touches the manifest layout, re-run `claude plugin validate --strict`
against the actual repo (not a fixture) before trusting any restructure — this
exact failure mode is easy to reintroduce.

### What could NOT be verified this session — needs different access

The Success Criteria's interactive checks — `claude --plugin-dir <path>` loading
the plugin, `/reload-plugins` picking it up, and **the skill actually triggering
on a natural task-submission-shaped prompt** — could not be driven end-to-end
from inside this session. This environment's permission classifier denies a
nested `claude -p`/interactive spawn from within an already-running Claude Code
session (confirmed live during KT2 — the attempt was blocked outright, not
merely slow). What *was* verified as a substitute, and is solid evidence but not
the same thing:

- `claude plugin validate --strict` clean on the real plugin.
- `claude plugin marketplace add` + `claude plugin install` + `claude plugin
  list` + `claude plugin details lopi` all succeed and show the expected
  component inventory (1 skill, 1 MCP server, ~117 tok always-on cost).
- The installed binary at its real cache path (`.../lopi/<version>/bin/lopi`)
  round-trips a real `initialize` → `lopi_submit_task` → `lopi_get_task` MCP
  session correctly.

**What a session with a real interactive `claude` — a human's local machine, or
an environment that doesn't block nested `claude -p` — needs to check:**
`claude --plugin-dir <path-to-lopi-repo>` (or the marketplace-installed
version), then in a live session ask something task-submission-shaped ("submit
a lopi task to fix X") and confirm the `lopi-cli` skill actually fires and picks
the right MCP tool, not just that the skill *would* structurally load. Also
confirm `/reload-plugins` after a manifest edit actually picks up the change
without a full session restart.

### Also flagged, not blocking

`LOPI_VS_OPENCLAW.md`'s feature table (row 2, "Agent Loop") cites an
`AgentState` enum with `Planning → Implementing → Testing → Scoring →
OpeningPr → RollingBack` transitions. The real `AgentState`
(`crates/lopi-core/src/agent.rs`) has no `OpeningPr`/`RollingBack` variants and
is constructed nowhere in the codebase — dead scaffolding. `TaskStatus`
(`crates/lopi-core/src/task.rs`) is the live, CLI/API-surfaced type.
`skills/lopi-cli/SKILL.md` documents `TaskStatus` and calls out the drift
inline; `LOPI_VS_OPENCLAW.md` itself is still stale and worth a small fix
later — not this sprint's job, didn't touch it.

### Explicitly not started (non-goals, correctly)

Track B (MCPB desktop extension) and Track C (Connectors Directory) — neither
touched. Track B reuses the exact same `ToolHandler` and state-sharing design
(see `LEDGER.md`); Track C needs its own re-derivation, not a copy-paste of
KT4's answer (a Streamable HTTP transport serving multiple concurrent clients
changes the dispatch-ownership calculus — see `LEDGER.md`'s closing note).
