# Ledger

A running log of load-bearing design decisions — the ones that would be
expensive to silently re-litigate in a later sprint. One entry per sprint,
newest first. Not a changelog (that's `CHANGELOG.md`) — this is *why*, not
*what*.

## Git hygiene — fixed the committed DRY violations (`dry_check.py`: 794 → 12)

**Starting state confirmed, then a delta reported before fixing:** the last
"Gate verification" note named four offenders (the `api_plan.rs`/
`stability/mod.rs` Task-builder pair, the `lopi-git` worktree/rebase test
overlap, `dlq_handlers.rs`, `task_stream_handlers.rs`). Running `dry_check.py`
fresh found **46 file pairs / 794 raw window-matches** — the four named
offenders were all still present, but so were ~40 more pairs never
individually named (same-file internal repetition in several crates, and a
large `lopi-ui` test-boilerplate cluster). Fixed in priority order below;
final state is **12 raw matches across 4 file pairs (3 distinct justified
reasons — `dag.rs` accounts for two of the four pairs under the same sqlx-
boilerplate reasoning)**, each a documented residual — not silently accepted,
each has a concrete structural reason `dry_check.py` cannot see.

**De-duplicated (real fixes, one source of truth each):**
- `api_plan.rs`/`stability/mod.rs` test-builder pair → `lopi-agent::test_support::make_test_task`, itself simplified to delegate to `Task::new` instead of re-listing all 20 fields.
- `api_plan.rs::build_user_prompt` / `stability::build_stability_prompt` (a *second*, previously-unnamed duplicate between the same two files — real production prompt-building logic, not test code) → shared `lopi-agent::prompt::build_user_prompt`; `build_stability_prompt` is now a one-line delegate. The original author's comment ("kept standalone to avoid coupling to the private `api_plan` module") is resolved by the new module living at the crate root, not inside `api_plan`.
- `dlq_handlers.rs`, `task_stream_handlers.rs` (self-duplicate 404/500 response bodies, and a repeated log-row→JSON mapping) → `dlq_not_found`/`dlq_internal_error`, `log_rows_to_json`/`logs_internal_error`.
- `crates/lopi-agent/src/runner/run_loop.rs` (self-duplicate rollback+checkout, 7×, and rollback+status(Retrying), 3×) → `abort_attempt` free fn + `AgentRunner::abort_and_mark_retrying` method.
- `crates/lopi-context/src/window.rs` (self-duplicate auto-evict-toward-threshold block in `push`/`push_tool_pair`) → `ContextWindow::evict_toward_threshold`.
- `crates/lopi-core/src/config_tests.rs` (self-duplicate temp-TOML-file test setup) → `write_temp_lopi_toml` + `temp_config_with_report_channel`.
- `crates/lopi-git/src/worktree.rs` (`run_git`/`run_git_stdout` self-duplicate) → `run_git` now delegates to `run_git_stdout`.
- `crates/lopi-orchestrator/src/scheduler.rs` (self-duplicate `ScheduleEntry` test fixtures, 3 pairs) → `make_entry` helper.
- `crates/lopi-remote/src/whatsapp.rs` ↔ `crates/lopi-ui/src/web/api_middleware.rs` (byte-identical `constant_time_eq` — security-relevant, genuinely dangerous to drift) → `lopi_core::security::constant_time_eq`, one implementation for both crates.
- `crates/lopi-remote/src/whatsapp.rs`, `crates/lopi-webhook/src/github.rs` (self-duplicate axum test-request boilerplate) → `post_webhook` helper in each crate's own test module (kept separate — see residual note below on why these two crates can't share one).
- `crates/lopi-spec/src/lib.rs` (self-duplicate extractor-dispatch-and-tag-error-handling for `.rs`/`.py` branches) → `scan_with` helper.
- `crates/lopi-spec/src/{rust_extractor.rs,python_extractor.rs}` (byte-identical `name_to_description`) → moved to the crate root, both modules import it.
- `crates/lopi-toon/src/lib.rs` (byte-identical "spec example" JSON fixture in two tests) → `spec_example()` helper.
- `crates/lopi-toon/src/encode/helpers.rs` (`encode_scalar_value`/`encode_cell` identical but for one bool) → shared `encode_scalar_common(v, delim, in_cell)`.
- `crates/lopi-toon/src/decode/parser.rs` (self-duplicate "parse remaining object fields at depth+1" loop in two `parse_array_body` branches) → `Parser::parse_remaining_object_fields`.
- `crates/lopi-ui/src/web/{tests.rs,tests_extended.rs}` — by far the largest cluster (**593 of the original 794 raw matches**): both files are `include!()`-ed into one module, so a single `get_req`/`send_req`/`test_app_with_store` helper trio (added to `tests.rs`) resolved the entire cross-file and self-file axum test-request boilerplate at once. Two Python scripts did the mechanical call-site rewrite (regex-matched the exact `Request::builder()...oneshot()...unwrap()` shape); every rewritten test was individually re-run green before and after.
- `crates/lopi-context/tests/tool_pair_atomicity.rs` (self-duplicate `push_tool_pair(make_msg(...), make_msg(...))` fixture, 4×) → `push_pair` helper.
- `crates/lopi-context/tests/{phase_eviction.rs,conclusion_preservation.rs,budget_lifo.rs,tool_pair_atomicity.rs}` (four different-arity `TaggedMessage` builders, all re-listing the same 9-field literal) → `tests/common/mod.rs` (the standard Rust idiom for code shared across integration-test binaries), each file's own narrower helper now delegates to `common::make_msg` with its fixed defaults.
- `web/src/lib/*.test.ts` (9 files: `api`, `badges`, `excitement`, `events`, `markdown`, `agentReducer`, `transcript`, `layout-core`, `session-groups`) all hand-rolled the same pass/fail-counter + `eq`/`ok` assertion harness (two variants: `Object.is` and `JSON.stringify` comparison) → `web/src/lib/test-harness.ts`, exporting a `record` primitive plus `eq`/`eqIs`/`ok`/`summary`/`namedSummary` built on it. Files needing the `Object.is` variant import `eqIs as eq` (aliased, so call sites didn't need touching); files with a custom approx-comparator (`excitement.test.ts`'s `close()`) call the new `record` primitive directly instead of mutating raw counters (which import bindings can't do). Every one of the 9 files was individually re-run via `npx tsx` before and after, plus a full `npm run check` — all pass, 0 TS errors.

**Left as documented residuals (4 file pairs, 12 raw matches, 3 distinct reasons) — not fixed, with why:**
- **`crates/lopi-git/src/worktree/tests.rs` ↔ `crates/lopi-git/tests/rebase.rs`** (identical `fn git(repo, args)` test helper). Structural, not fixable without a worse trade: `worktree/tests.rs` is a `#[cfg(test)] mod` compiled *inside* the library crate (`use super::*` gives it access to private items like `worktree_slug`/`add_args`), while `tests/rebase.rs` is a separate integration-test binary with only the crate's public API. Rust has no shared-code mechanism between those two contexts short of making the helper `pub` (pollutes the public API for a test-only convenience) or adding a new dev-only shared crate (out of scope — "no new dependency").
- **`crates/lopi-memory/src/store/{dag.rs,q_routing.rs,verifier.rs}`** (identical `.fetch_all(&self.read_pool).await?; Ok(rows) }` tail + adjacent `#[cfg(test)] mod tests` preamble). Each function queries a different table into a different row type (`DagNodeRow`, `RoutingQValueRow`, `VerifierVerdictRow`); the only thing matching is how any `sqlx` `fetch_all` call necessarily ends. No real abstraction exists here without genericizing over the query and row type, which sqlx itself already is the abstraction for.
- **`crates/lopi-remote/src/whatsapp.rs` ↔ `crates/lopi-webhook/src/github.rs`** (the `#[cfg(test)] #[allow(...)] mod tests { use super::*; use axum::{ ... }` preamble). Pure boilerplate common to any axum-handler test module in this codebase — not meaningfully shared logic, and coupling two unrelated crates' test preambles together to satisfy a textual match would be exactly the "contort real code" the brief warned against.

`dry_check.py` was NOT run with any scoped ignore/allowlist (the tool has none — checked its full source: no per-pair suppression mechanism exists, only `--staged-only`/`--changed-only`/`--warn-only` mode flags). The residual above is accepted at the repo level, documented here per the brief's fallback option.

**Decision:** dropped the local worktree-isolation stash created before this
session's sync with `origin/main`. `origin/main`'s own `WorktreeManager`
(RAII `Worktree`, slug-based naming, `WT_META_LOCK`, `gc`/`list`/`prune`,
`pool/mod.rs` + `pool/worktree.rs` split) is the kept implementation —
confirmed, not assumed, more capable than the stashed version, which had no
equivalent for `gc`/orphan-detection and split its capability across a
single-file `pool.rs`.

**Redundancy proof (21 of 25 stash files):** every stash file mapped to an
`origin/main` file/mechanism implementing the same capability — see the
full file-by-file table produced during this pass. Two design-surface
differences noted but not blocking: (1) main's `LoopConfig.isolation:
IsolationMode` is a simpler enum toggle vs. the stash's `WorktreeConfig`
(configurable root/base-ref/cleanup-age) — same core capability, less
configurable; (2) `add_detached` branches from local `HEAD` unconditionally,
where the stash had a `BaseRefPolicy::RemoteHead` default — a real behavioral
difference, judged non-blocking since the overall architecture choice
(main's `WorktreeManager`) was already decided, not something this pass
re-opened.

**What was NOT superseded (2 files, different severity):**
- `crates/lopi-ui/src/web/worktree_handlers.rs` (`GET /api/worktrees`) — no
  web-exposed worktree listing exists anywhere on `main` today; CLI parity
  exists (`src/worktree_commands.rs::{list,gc}`). Minor, accepted as a gap
  rather than salvaged, since the underlying capability is reachable via CLI.
- **`docs/ui/{lopi-loop-stacks-3-output,lopi-scope-and-test-plan,lopi-selectors-panes}.html`**
  — the actual design mockup source material `UI_PLAN.md` (already merged)
  was written against. Unrelated to worktree isolation; only present in this
  stash because the original `git stash push` swept up everything uncommitted
  at the time. **Extracted before the drop** (`git checkout stash@{0} --
  docs/ui/`) and left staged, uncommitted, for separate review — not lost.

**Honest DRY-gate outcome — do not overstate:** the stash was never applied
to the working tree, so it could not have been contributing to
`dry_check.py`'s failures in the first place. Proven directly: ran the check
before the drop (stash present but unapplied) and after (stash gone) — the
failing-file set is byte-identical both times (`diff` exit 0). **Dropping
the stash changed nothing about the DRY gate.** The gate still fails on
committed code — the same pre-existing set recorded in the prior "Gate
verification" entry (`api_plan.rs`/`stability/mod.rs` test-builder pair,
`lopi-git` worktree/rebase test overlap, `dlq_handlers.rs`,
`task_stream_handlers.rs`, and others) — which remains its own, separate
cleanup, not addressed by this pass. `cargo test --workspace` (704
passed/1 failed, the same pre-existing unseeded `qlearned_favours_highest_
reward_member` flake) and `cargo clippy --workspace -- -D warnings` (clean)
confirm dropping the stash broke nothing, as expected since it was never
applied.

## Sprint 5 — Expose Loop Fields on `CreateTaskRequest` (`crates/lopi-core/src/task.rs`, `crates/lopi-ui/src/web/{types.rs,handlers.rs}`, `crates/lopi-agent/src/claude.rs`, `crates/lopi-orchestrator/src/pool/run_loop.rs`)

**Gate verification (evidence, not assertion) — merge-prep pass:**

- **`dry_check.py`** fails on both this branch and clean `origin/main`. Proof:
  stashed the branch's tracked changes (working tree then byte-identical to
  `origin/main`, confirmed via `git diff origin/main --quiet`), ran the
  checker, restored the stash, ran it again. File-level failing set: identical
  (`diff` exit 0). Pair-level failing set (`fileA ↔ fileB`, line numbers
  stripped so this branch's line-shifts don't mask a real comparison): **46
  pairs on origin/main, 46 on the branch, `comm -13`/`comm -23` both empty —
  zero pairs added, zero removed.** This branch adds no new duplicate.
  Confirmed separately: exactly one definition each of `ReportChannel::parse`
  (`report.rs:43`), `select_model` (`claude.rs:45`), `resolve_verifier`
  (`verifier.rs:34`) — every call site reuses the one definition.
- **`npm run check`** originally reported 7 errors, all in `markdown.ts`/
  `highlight.ts`/`parser.test.ts` (never touched by this branch) importing
  `marked`/`dompurify`, which were listed in `package.json` but never
  installed in this checkout. After `npm install` (53 packages): **0 errors**,
  2 pre-existing warnings in files this branch never touched
  (`HelpOverlay.svelte` a11y, `fleet/+page.svelte` CSS). `api.ts` — this
  branch's only frontend change — was clean before and after.
- **`cargo test --workspace`** (nextest unavailable in this environment,
  same as the prior session — used plain `cargo test`): 704 passed, 1 failed.
  The failure, `constellation::tests::qlearned_favours_highest_reward_member`,
  is an **unseeded statistical test** (200 ε-greedy Q-learning trials against
  a `b_count > 120` threshold, no fixed RNG seed — a pre-existing violation of
  this repo's own "seed everything stochastic" rule). Confirmed flaky by
  direct measurement: 5 isolated reruns, 1 failure (20%), with zero code
  changes. Confirmed unrelated to this branch: `git diff origin/main --stat --
  crates/lopi-orchestrator/src/constellation* crates/lopi-orchestrator/src/q_router.rs`
  is empty — this branch has never touched that code. Not fixed here (out of
  this sprint's scope); flagged as its own follow-up rather than silently
  re-run until it happened to pass.
- **`clippy --workspace --all-targets -D warnings`**: clean. **`RUSTDOCFLAGS=
  "-D missing_docs" cargo doc --no-deps --workspace`**: exits 0 (pre-existing
  `rustdoc::broken_intra_doc_links` warnings on `TopologyHint`/`StreamEvent`/
  `types`/`JobScheduler` are warnings, not `missing_docs` errors, and none are
  in this branch's new fields' doc comments). No reference to the old
  `select_model` signature (`-> &'static str`) survives anywhere in the
  workspace — grepped explicitly.

**Decision (`max_iterations: 0` is the infinite-loop sentinel — a one-way
door):** `Task.max_iterations: Option<u8>` uses `0` to mean "no cap," not an
`Option`-based ∞ or a separate boolean. This was chosen deliberately over the
`Option` alternative (locked in per the sprint brief) and matches the "0 =
disabled/unbounded" convention `LoopConfig` already uses for
`no_progress_limit` and `budget_tokens` — no new convention introduced.
**One-way-door consequence:** every consumer of `AgentRunner.max_turns` had to
be audited for "0 means unlimited" rather than "0 means immediately expired."
Two call sites got this wrong by default and were fixed as part of this
sprint: the hard-stop check in `runner/run_loop.rs` (`turn_count > max_turns`
would have fired on the very first turn) and the CLI flag pass-through
(`ClaudeCode::with_max_turns` would have sent a literal `--max-turns 0` to
the real `claude` subprocess). Both now special-case `max_turns == 0` to skip
the cap/flag entirely. Any future code that reads `max_turns` must do the
same — there is no compiler enforcement of this invariant.

**Decision (scope expanded from "expose existing fields" to "add two new
`Task` fields"):** the sprint brief's original ask was pure surface exposure
— wire already-tested fields through to the web API. Recon before writing
any code found that `Task.model`/`Task.effort` had **no existing backing at
all** (`select_model` is a pure heuristic reading nothing stored; "effort" is
a verifier-only concept) and `max_iterations` lived only on the repo-level
`LoopConfig`, never on `Task`, with no per-task override precedent. Exposing
these as dead `CreateTaskRequest` fields with nowhere to bind would have been
worse than not exposing them — silent, misleading surface. Flagged to the
user before writing code; explicitly authorized to add the two new `Task`
fields plus the minimal read-side wiring, rather than silently inventing
fields or silently dropping them from scope.

**Decision (worker `effort` is stored, not yet folded into any prompt):**
unlike `verifier_effort` (folded into the verifier's system prompt via
`build_system_prompt`), `Task.effort` has no equivalent fold point for the
worker. The direct-API planning path's system prompt
(`api_client::LOPI_SYSTEM_PROMPT`) is `cache_control: ephemeral` and must
stay byte-identical across a task's retry loop to keep its ~90% cache-hit
rate (see Sprint G's doc comments in `runner/api_plan.rs`) — folding a
per-task hint into it would silently regress that optimization. Rather than
invent a fold point under sprint pressure, `Task.effort` is stored
(round-trips through the API, survives serialization) and left unconsumed;
folding it in is a deliberate follow-up design pass, not a default assumed
here.

**Decision (task-level override always wins, mirroring `verifier_model`):**
`build_runner`'s `max_turns` resolution is `task.max_iterations.unwrap_or(repo_max_iterations)`
and `select_model` checks `task.model` before any heuristic — both follow the
"explicit wins over default" precedent Sprint 4 already established for
`verifier_model`, rather than inventing a new precedence rule.

**Fixed in passing (was a latent gap, not introduced by this sprint):**
`LoopConfig.max_iterations` was loaded by `run_one` (for a tuple destructure)
but never actually applied to `AgentRunner.max_turns` — any repo customizing
`.lopi/loop.toml`'s `max_iterations` had that setting silently ignored.
Closed as part of wiring the task-level override, since both needed the same
plumbing. Also fixed in passing: the blocking `LoopConfig` load's `JoinError`
fallback used `.unwrap_or_default()` silently (a `no-silent-failures` gap) —
now logs via `tracing::warn!` and falls back to `LoopConfig::default()`
explicitly, so `max_iterations` lands on its safe default (25) rather than
`u8::default()` (0 — the new infinite sentinel) in that rare failure path.

## Sprint 4 — Verifier as Explicit Gate (`crates/lopi-agent/src/verifier.rs`, `crates/lopi-agent/src/runner/verifier_runner.rs`, `crates/lopi-core/src/{loop_config.rs,task.rs}`, `crates/lopi-orchestrator/src/pool/run_loop.rs`)

**Decision (never-grade-your-own-homework default):** when `verifier_model` is
unset, the resolved verifier model must differ from the worker model that
produced the diff being graded. Documented default: **Opus**, unless the
worker itself already ran on Opus (an escalated retry, `attempt >= 2` per
`select_model`), in which case the verifier falls back to **Sonnet** instead.
This is a pure function, `lopi_agent::verifier::resolve_verifier(worker_model,
verifier_model, verifier_effort) -> (model, effort)`, unit-tested in isolation
— it is the one place this rule is enforced, so `run_verifier_pass` never
duplicates the logic. An *explicit* `verifier_model` is always honored as-is,
even if it happens to equal the worker's model — that's a deliberate operator
override, not a default, and enforcing "different" there would silently
override a user's stated choice.

**Decision (effort is a prompt hint, not a wire parameter):** `verifier_effort`
threads into `VerifierAgent::verify`'s system prompt as a plain-text
`"Reasoning effort: {effort}"` line, the same convention the web cockpit
already uses for worker-side launch controls (`web/src/lib/stores/agents.ts`
folds its `effort` selector into a planning constraint the same way — see
`CHANGELOG.md`'s "Model / effort / priority / repo / branch selectors" entry).
The Anthropic API client (`AnthropicClient::complete`) has no reasoning-effort
request parameter at all — only a token-based `task_budget` (Phase 16.6),
which is a different mechanism (self-pacing, not reasoning depth). Inventing a
wire-level parameter that doesn't exist would be scope creep beyond "activate
and parameterize" the existing VerifierAgent; folding it into the system
prompt text reuses an established pattern instead of adding a new one.

**Decision (the pool-construction seam):** `run_one`'s runner-builder chain
was extracted into `build_runner` — a pure assembly function (no I/O) that
takes every already-resolved input and returns the configured `AgentRunner`,
calling `.with_verifier()` when `task.verifier_required ||
task.verifier_model.is_some()`. This is the load-bearing kill-test seam
(Capability 2's kill-test, `PROMPTS_PLAN.md`): a unit test builds a `Task`
with `verifier_required = true` and an `AutonomyLevel::DraftPr` (L2, which
alone would *not* force the verifier) and asserts the resulting
`AgentRunner::verifier_enabled()` is `true` — without ever calling `.run()`,
so the never-before-exercised maker/checker flow is proven wired without
actually executing it. `AgentRunner::verifier_enabled()` (a `pub const fn`
getter) was added for exactly this assertion; the field itself
(`AgentRunner.verifier_enabled`) already existed but had no external reader.

**Why the seam, not a network-level assertion:** `PROMPTS_PLAN.md`'s literal
kill-test wording ("assert the client received SONNET, not OPUS") implies
intercepting the outbound HTTP call, but `AnthropicClient` has no
base-URL injection point and the workspace has no HTTP-mocking dependency.
Adding one would be a new third-party dependency and a wire-level change to
`AnthropicClient` — both outside this sprint's pre-authorized scope ("REUSE
[VerifierAgent] AS-IS... this sprint only activates and parameterizes it").
The equivalent, dependency-free proof: `resolve_verifier` (the only place a
model gets chosen) is unit-tested directly, and `verify`'s body — visible in
the diff this sprint prints — has zero remaining reference to a hardcoded
model constant; the `model: &str` parameter flows straight into `.complete()`
with no branch in between.

**What now exercises the previously-dead `.with_verifier()` path:** any task
or `.lopi/loop.toml` that sets `verifier_required = true` or a
`verifier_model`, submitted through `AgentPool::submit` → `run_one` →
`build_runner`. Before this sprint the only way to force the verifier was
`autonomy_level >= VerifiedPr` (L3/L4); that mechanism is untouched
(`requires_verifier` in `finalize.rs` still ORs both together at finalize
time). The first time this call site runs in production will be the first
real, live exercise of `VerifierAgent`'s maker/checker isolation outside its
own unit tests — treat an early failure there as expected discovery, not a
regression.

**Housekeeping:** two existing test-only `Task { .. }` struct literals
(`crates/lopi-agent/src/runner/api_plan.rs`, `crates/lopi-agent/src/stability/mod.rs`)
needed the three new fields added to compile; `dry_check.py` still flags
these two helpers as near-duplicates of each other (pre-existing, unrelated
to this sprint — both already duplicated the full `Task` literal before this
change) and unrelated pre-existing duplication elsewhere in the workspace
(`lopi-webhook`, `lopi-spec`, `lopi-remote`). No verifier logic itself is
duplicated anywhere — `resolve_verifier` and the one `.with_verifier()` call
site are each defined exactly once.

**How to apply:** any future "gate" field that should be forceable
independent of `autonomy_level` should follow this same shape — a bool +
optional override(s) on both `LoopConfig` and `Task`, `#[serde(default)]`,
read at the pool-construction seam rather than threaded through `.lopi/loop.toml`
at runtime (Task is the authoritative per-run source, matching how
`autonomy_level` already works — `LoopConfig`'s copy is the UI-editable
repo-level default/display value, not something `run_one` re-reads
automatically). Any future "resolve a value that must differ from another
value" pattern should follow `resolve_verifier`'s shape: a pure function,
unit-tested in isolation, called from exactly one production site.

## Sprint 3 — Report on Finish (`crates/lopi-core/src/{report.rs,config.rs,task.rs,event.rs}`, `crates/lopi-agent/src/runner/finalize.rs`, `crates/lopi-remote/src/telegram/notify.rs`)

**Decision (dependency edge):** neither pre-authorized edge (`lopi-agent` →
`lopi-remote`, or a trait-in-core) was taken. Reading the actual dep graph
first showed `lopi-remote` already depends on `lopi-orchestrator`, which
depends on `lopi-agent` — so `lopi-agent` → `lopi-remote` would have been a
real cycle, exactly the failure mode `NEXT.md` flagged up front. Instead,
`AgentEvent` (already in `lopi-core`, already depended on directly by both
`lopi-agent` and `lopi-remote`) gained one new variant, `ReportReady { task_id,
channel, summary }`. `emit_report` broadcasts it on the existing
`EventBus<AgentEvent>`; `lopi-remote`'s already-running `notify_loop` gained
one new match arm that calls the existing `send_msg` helper. Net new
dependency edges: **zero** — `cargo tree -p lopi-agent` / `-p lopi-remote`
are unchanged, no `Cargo.toml`/`Cargo.lock` edits at all. This is a stronger
fit than either pre-authorized option: it needed no new abstraction (the
event-bus *is* the report-sink seam) and no cross-crate call.

**Decision (chat_id):** option (a) — the report reuses the single global
`remote.telegram.chat_id` this loop was booted with. `notify_loop`'s existing
gate (`return` when `chat_id` is `None`) is untouched; `ReportReady` just adds
another event the existing `chat_id: ChatId` in scope can be sent to. **Known
limitation:** every `report = "telegram"` schedule in a given `lopi` process
notifies the same chat — there is no per-task destination yet. Building
per-task routing (option b — `ScheduleEntry` carrying a target chat id) was
explicitly out of scope this sprint (`NEXT.md`: "do NOT build a full per-task
routing system"); revisit if/when multiple distinct Telegram destinations are
needed.

**Decision (channel validation):** `report: Option<String>` (not a typed enum
field) on both `ScheduleEntry` and `Task`, per `NEXT.md`'s explicit call —
threaded from `ScheduleEntry` to `Task` in `scheduler.rs` the same one line as
`autonomy_level`. The typed side is `ReportChannel::parse(&str)` in the new
`lopi-core::report` module: `"telegram"` parses; `"whatsapp"` is a *named*
`WhatsappUnsupported` error (inbound-only Twilio webhook, no send path — not
lumped in with generic `Unknown`); anything else is `Unknown(name)`. Called
in two places, both reusing the same `parse` fn (no second scanner): (1)
`LopiConfig::load()` validates every `[[schedules]]` entry's `report` and
fails the whole load loudly on a bad channel — a typo'd config never silently
never-sends; (2) `emit_report` re-validates defensively (a `Task` can reach
`emit_report` from sources other than `ScheduleEntry`), `tracing::warn!`-ing
and skipping the broadcast rather than sending an unrecognized channel name.

**Why:** the config-load validation is the one guaranteed choke point — every
`ScheduleEntry` a user writes passes through it, so it is where a typo must be
caught, not where it's merely convenient to catch it. Re-validating at
`emit_report` costs one extra `match` and closes the gap for tasks built
outside the schedule path (API, CLI) that could carry an unvalidated `report`
string directly.

**Housekeeping:** `crates/lopi-core/src/event.rs` was already at 590 lines
(over the 500-line hard gate) before this sprint; adding `ReportReady` pushed
it to 621. Since the file-size CI gate scans *changed* files on a PR, this
sprint's edit would have tripped it. Split the file's two `#[cfg(test)]`
modules out to `event_tests.rs` / `event_wire_format_tests.rs` via the
`#[path = "..."]` pattern already used by `config_tests.rs` /
`loop_config_tests.rs` — a pure test-relocation, zero logic changes — bringing
`event.rs` itself to 323 lines. Same category of proactive split as
`run_loop.rs`'s (Sprint 2 era), just triggered by an existing-debt file this
time rather than new code.

**How to apply:** any future `lopi-agent` → `lopi-remote` (or similarly
"downstream" crate) communication should default to an `EventBus<AgentEvent>`
variant before reaching for a new dependency edge or a bespoke trait —
check `cargo tree` for the real graph first, since a plausible-looking direct
call can be a cycle in disguise. Any new `report`/channel-shaped field should
validate through `ReportChannel::parse`, not a second name-matching branch.

## Sprint 2 — Skill Arguments (`crates/lopi-skill/src/{lib.rs,invocation.rs}`)

**Decision:** empty `args` on a body containing `$ARGUMENTS` is an **empty
fill, not an error** — `$ARGUMENTS` becomes `""`, and rendering still
succeeds. And: `render_body` reuses `template::resolve` by *translating*
`$ARGUMENTS` → `{arguments}` and calling `resolve` with a one-entry
`{"arguments": args}` vars map — no second `.replace()`/scanner, per Sprint
1's hard reuse constraint. `Skill` needs no new frontmatter field for this;
`$ARGUMENTS` lives in the existing body `String`.

**Why:** an empty-fill (not an error) is the least-surprising choice —
`:kcqf` alone (no argument) is a legitimate, common invocation shape, and
`resolve` itself already treats a *present* vars entry mapped to `""` as a
perfectly valid substitution (this is distinct from a *missing* key, which
is still the loud `TemplateError` Sprint 1 built). Erroring on empty args
would penalize the common case for no real safety gain. On reuse: the
translate-then-delegate approach was chosen over extending `resolve` with a
second hole syntax (`$NAME`) because it needed **zero changes** to
`template.rs` — the smallest change that could possibly work, and it
composes: any future skill-body placeholder can follow the same
translate-to-`{hole}` pattern without `template.rs` ever learning a second
syntax. The tradeoff this creates: a skill body with a genuinely stray,
unescaped `{` (not part of `$ARGUMENTS`) will error on invocation, exactly
as a hand-written template would — skill authors get Sprint 1's `{{`/`}}`
escape rule "for free," not a more lenient bespoke rule.

**How to apply:** any future skill-body placeholder should translate to a
`{hole}` and delegate to `resolve`, not add new substitution logic. If a
skill body needs to contain a literal, un-doubled `{` going forward, that's
now a real authoring constraint worth documenting in the skill-writing docs,
not a bug in `render_body`.

## Sprint 1 — Prompt Templates (`crates/lopi-core/src/template.rs`)

**Decision:** escaping follows Rust's `format!` rule — `{{` and `}}` decode to
a literal `{` / `}`, independently of hole-matching (not a paired
`{{...}}` block). And: stop at a bare `resolve()` fn — no `PromptTemplate`
newtype.

**Why:** the escape rule is copied wholesale from a convention every
Rust contributor to this repo already knows (`format!`/`println!`), so there's
no new grammar to learn or document — `{{brace}}` reads as "the same rule as
`format!`" instead of a bespoke invention. The fn-vs-newtype call: a newtype
would only earn its keep once templates carry state beyond the string itself
(a source location, a cached parse, validation metadata) — none of which this
sprint's four call sites need. Building it now would be exactly the kind of
premature abstraction CLAUDE.md warns against; the moment a second sprint
needs more than a `&str` in, `String` (or `Result`) out, promote it then.

**How to apply:** any future sprint that touches template syntax (nested
holes, default values, conditional holes) must extend this same escape rule
rather than introducing a second one — and should re-examine the newtype
question at that point, not before.
