# 00 · Current-state map — the real code behind Track A/B

**Method:** every claim cites `file:line` from the working tree at commit `12de652`. Where
this contradicts the roadmap or a design doc, it is flagged **[DOC-DRIFT]**. Read this
before any design section — it is the ground truth the specs build on.

> **Headline correction.** The roadmap says lopi "currently has *no evaluator running at
> all*" (`docs/lopi-loop-intelligence-roadmap.md:38`). **That is wrong.** A separate-model
> judge — the Konjo Verifier — is fully built and wired, calls a second model today, and
> gates finalization at L3/L4. The real gap is narrower and more precise than "no
> evaluator": see §1 and §2 below. This correction reshapes A1 from "build a judge" into
> "generalize the judge lopi already has into a tiered eval executor with an explicit goal
> object." **[DOC-DRIFT]**

---

## 1. Evals today — client-only intent, no executor

- An "eval" is a two-field tag, not an executable check: `EvalRef { name: string, tier:
  EvalTier }` where `EvalTier = 'base' | 'test' | 'judge' | 'suite'`
  (`web/src/lib/stores/stack.ts:21`, `:24-27`).
- The catalog is presentation only: `BASELINE_EVAL = {name:'execution ok', tier:'base'}`
  (`stack.ts:152`), `EVAL_CATALOG` of 9 pickable evals incl. `beats-best`, `30-run gate`,
  `code review`, `vuln scan`, `adversarial` (`stack.ts:156-167`), preset→eval seeding in
  `PRESET_CATALOG` (`stack.ts:177-233`), attached to a card by `buildCard()`
  (`stack.ts:316`).
- **Confirmed: no executor anywhere.** `evals` is never serialized to the backend —
  `cardToTaskPayload()` maps model/effort/iterations/on_fail/gate/until/client_ref and
  **omits `evals`** (`stack.ts:586-608`); `web/src/lib/api.ts` has zero `evals` references.
  A word-boundary search for `eval` across `crates/` returns one comment
  (`crates/lopi-agent/src/runner/postmortem.rs:15`) — no eval field, no dispatcher, no
  tier executor in Rust. The code says so itself: "CLIENT-ONLY (chain-acceptance intent
  only; **eval execution doesn't exist anywhere yet**)" (`stack.ts:766-768`).
- **The eval tiers are not the `Scorer`.** `Scorer::score()` runs a hardcoded
  `cargo test --quiet` + `cargo clippy -D warnings`, else `npm test`, else "no test runner
  detected → treat as passing" (`crates/lopi-agent/src/scorer.rs:28-91`). It reads no eval
  list; the catalog's granularity (unit vs integration vs benchmark vs adversarial) has no
  executor behind it (`scorer.rs` never references evals). The `Score` it emits is
  `{ test_pass_rate, lint_errors, diff_lines, errors }` with `passed()` = all tests pass
  AND zero lint (`crates/lopi-core/src/agent.rs:116-160`). **Binary, not a scalar quality
  score** — `weighted()` produces a scalar but `passed()` is what gates the loop.

**So:** what looks like configurable acceptance criteria is inert client state that is never
transmitted. This is the single largest intent-vs-reality gap in the system.

## 2. The judge already exists — the Konjo Verifier

The separate-model judge the roadmap wants to "build" is built:

- `VerifierAgent::verify()` makes a real second-model API call
  (`crates/lopi-agent/src/verifier.rs:152-170`, `self.client.complete(model, …)` at
  `:164`) and parses a structured `VerifierVerdict { passed, gaps, fix_hints, confidence }`
  (`:169`, `:204-207`; type in `lopi-core`).
- **"Never grade your own homework" is implemented:** `resolve_verifier()` picks a model
  that differs from the worker — Opus by default, Sonnet when the worker is already Opus
  (`verifier.rs:34-47`), test-pinned (`verifier.rs:349-356`).
- **Maker/checker isolation is implemented:** `VerifierAgent` defaults `isolated: true`;
  `build_prompt()` omits the maker's plan entirely when isolated (`verifier.rs:120-128`,
  `:179-202`) so the checker grades the artifact (diff+goal+rubric+test output) without the
  maker's reasoning.
- **It is wired end-to-end.** Pool: `verifier_needed = task.verifier_required ||
  task.verifier_model.is_some()` → `runner.with_verifier()`
  (`crates/lopi-orchestrator/src/pool/run_loop.rs:349`, `:372-376`). Finalize forces it on
  for L3/L4: `requires_verifier(enabled, level) = enabled || level.requires_verifier()`
  (`crates/lopi-agent/src/runner/finalize.rs:49-51`; L3/L4 via
  `loop_config.rs:93-95`). On rejection it rolls back, marks `Retrying`, and returns `None`
  to continue (`finalize.rs:75-82`).
- **It already does proto-A2 reflection.** On a FAIL verdict, `run_verifier_pass()` appends
  `verdict.fix_hints` to `self.task.constraints` (deduped) for the next attempt's planning
  prompt (`crates/lopi-agent/src/runner/verifier_runner.rs:73-82`).
- **Persistence exists.** `save_verifier_verdict(task_id, attempt, verdict, model)` INSERTs
  into `verifier_verdicts`; `load_verifier_verdicts(task_id)` reads back ordered by ts
  (`crates/lopi-memory/src/store/verifier.rs:38-82`).
- **Rubric resolution exists:** inline `Task::rubric` → `.konjo/rubrics/*.toml` →
  hardcoded default (`verifier.rs:70-108`).

**Two load-bearing caveats for A1:**
1. **The judge only runs at the finalize gate on already-*passing* work.** It is a
   maker/checker double-check after `Score::passed()`, not a general evaluator that decides
   pass/fail from scratch across tiers. There is no "run the judge as the primary verdict"
   path, and no goal/acceptance object it checks against — it checks a *rubric*, which is a
   criteria list, not a machine-checkable success condition. **[DOC-DRIFT]**: the roadmap's
   A1 tier list ("baseline → test → judge → suite") does not exist as tiers; only the judge
   and a hardcoded test scorer exist, unconnected.
2. **The verifier is fail-open.** No `api_client` → skips and returns `true`
   (`verifier_runner.rs:21-23`); an API/parse error → `warn!` and returns `true`
   (`verifier_runner.rs:54-57`). So at L3/L4 a verifier that errors does **not** block the
   PR. A "verified PR" guarantee that fails open is a real hole to close in A1/A3.

## 3. The run loop (agent level) — where an eval would hook in

`crates/lopi-agent/src/runner/run_loop.rs`, `AgentRunner::run()` (`:30-483`). Per attempt:
- Stability pre-flight + `gate` pre-flight (`:36-47`), then `for attempt in 0..max_retries`
  (`:68`).
- Model routing `select_model(&task, attempt)` — escalates to Opus after first failure
  (`:71`).
- plan (direct-API or CLI, `:175-227`) → optional plan-approval gate (`:251`) → implement
  (`:272-280`) → diff-scope check (`:283-291`) → **score** `scorer.score()` (`:306`).
- **The eval seam is here:** after `scorer.score()` at `:306`, the loop decides pass via
  `score.passed() || until_satisfied` (`:363-364`). This `if` is exactly where a tiered
  eval verdict would replace/augment `score.passed()`.
- `check_until()` runs a user shell command as an independent exit-condition
  (`run_loop.rs:363`; `run_guard_command` in `loop_config.rs:241-249`).
- On pass → `finalize()` (the verifier + PR path, §2). On fail → in-place `fix` (`:381`),
  then adaptive-retry error capture with a self-prompt strategy
  (`Direct/Reflexion/SelfRefine/PlanThenAct`) framing the failure for the next attempt
  (`:426-442`; strategies in `self_prompt.rs`), then the **no-progress stall guard**
  (`:448-464`) and backoff (`:467`).
- **Already present, load-bearing for A2/A3:** self-prompt reflection framing
  (`:440-441`), no-progress stall halt (`update_no_progress_streak`,
  `finalize.rs:238-251`), `until` exit-condition, adaptive-retry error routing. A2/A3 are
  less "build from zero" than "make measured, persistent, and eval-driven."

## 4. The orchestrator loop — dispatch only

`crates/lopi-orchestrator/src/pool/run_loop.rs`:
- The pool **dispatches**; it has no iteration loop. `AgentPool::run()` pops tasks and
  spawns one worker per task (`:84-256`); `run_one()` is the single-task driver
  (`:390-496`). All iteration/stall/finalize lives inside `runner.run()`.
- `loop_config` is loaded here (`LoopConfig::load_from_repo` in `spawn_blocking`,
  `:411-432`) and mapped to the runner: `max_iterations`→`max_turns` (`:355`, `:368`),
  `gate`/`until`/`on_fail` (`:358-360`, `:369-371`), verifier (`:349`, `:372-376`). Task
  override wins over repo `.lopi/loop.toml`.
- **The post-run hook seam** (where an orchestrator-scope eval/score-history write belongs)
  is `run_one` at `:473-495` — after `runner.run()` returns it emits `TaskCompleted`,
  `mark_completed`, and `mine_patterns`. **`mine_patterns` is the only post-run learning
  hook today.**
- `max_iterations` is a per-run *turn cap*, not a re-dispatch count. **No automatic
  re-queue** — failures dead-letter (`push_dlq`, `:274-304`) with no auto-retry. Repeats are
  cron-only via `ScheduleManager` (`crates/lopi-orchestrator/src/schedule_manager.rs`,
  `fire()`→`pool.submit`).

## 5. Memory — what a reflection/score-history would actually use

- **kohaku / episodic / vector store: does not exist.** It is a codename in `PLAN.md:435`
  and `roadmap:45,106` only. **[DOC-DRIFT]** — any design that "reuses kohaku" must build on
  `MemoryStore` (SQLite). The de-facto episodic store is the `attempts` table.
- **Per-iteration score history exists as raw rows, no query for it.** `attempts` carries
  `score_test_pass_rate / score_lint_errors / score_diff_lines / outcome / attempt_num`
  (`crates/lopi-memory/src/schema.sql:10-21`); write `save_attempt()`
  (`store/mod.rs:187-219`); read per-task `run_attempts(task_id)` oldest-first
  (`store/run_trace.rs:88-99`), fleet-wide `recent_loop_attempts()`
  (`store/loop_health.rs:52-63`). **There is no "is the score improving / has it stalled
  over the last N attempts" API** — a ratchet/no-progress *query* must be built on
  `run_attempts`. The agent-level `no_progress` streak (`finalize.rs:238`) is in-memory and
  never persisted where the orchestrator can read it.
- **patterns** (`store/patterns.rs`): write `mine_patterns(task_id, goal)` (`:167-218`),
  `insert_postmortem_pattern` (`:139`), `annotate_pattern` (`:224`); read
  `find_similar_patterns(goal)` Jaccard≥0.3 top-5 (`:80-101`), `load_patterns` (`:107`),
  `compute_weight_adjustments` (`:256`). A "pattern" is a goal-keyword fingerprint +
  rolling `avg_attempts`/`success_rate`.
- **lessons** (`store/lessons.rs`): write `save_lesson(repo, category, content, task_id,
  score)` — **silently skips writes when `score < 0.6`** (`:36-64`, gate const `:27`); read
  `load_lessons(repo, limit)` (`:70-81`). Nearest durable "learning" surface for A2, but
  not per-iteration and not linked to a score trajectory.
- **verifier_verdicts** (§2), **trust_ledger** (earned autonomy: `record_clean_run` /
  `record_failed_run` / `record_revert`, `store/trust_ledger.rs:83-123`),
  **loop_health/run_trace** (read-only projections). Store list: `store/mod.rs:345-379`.

## 6. Task payload & model/effort resolution — where the judge model resolves

- `cardToTaskPayload(card, defaults)` (`web/src/lib/stores/stack.ts:586-608`) →
  `CreateTaskOptions` (`web/src/lib/api.ts:73-95`), which **already carries**
  `verifier_required?`, `verifier_model?`, `verifier_effort?`, `model?`, `effort?`,
  `gate?`, `until?`, `on_fail?`, `max_iterations?`, `client_ref?` — mirroring Rust
  `CreateTaskRequest` (`crates/lopi-ui/src/web/types.rs`).
- Model/effort resolve at `stack.ts:591-592`: `card.config.model ?? defaults.model`;
  same for effort and repo. Resolution chain is **per-card override → stack/pane default**
  (`StackDefaults` in `stackDefaults.ts:13-19`).
- **The judge model has a wire path but no config/UI/resolution.** `cardToTaskPayload`
  never populates `verifier_*` (`stack.ts:590-599`); no stack/card control sets them
  (`StackConfigPopover.svelte:27-44`, `ConfigDrawer.svelte:27-73`). The `'judge'` eval tier
  is inert metadata with no model behind it. To give the judge its own model, add a field
  on `StackDefaults`/`CardConfig`, a resolution line at `stack.ts:591-592` populating
  `options.verifier_*`, and a control in `StackConfigPopover`. **Everything downstream of
  the wire already exists** (§2).

## 7. The stack sequencer — fixed-count, no goal

`web/src/lib/stores/stackRun.ts` (100% client-side; no server-side stack concept, `:5-13`):
- `advance()` (`:131-223`) is a `for(;;)` launching one card at a time. Chain repeat is the
  only "advance-N" mechanism: at `cursor >= order.length` it computes `moreRepetitions =
  loopTarget === 0 || nextRepetition < loopTarget` (`:149-160`). **Fixed-count** — a chain
  stops by exhausting `loopTarget` (0 = infinite), a failure policy, or pause/drain. **No
  goal/acceptance-based termination.**
- Launch snapshot in `runStack()` (`:230-255`): freezes `order`, `loopTarget`
  (`= pane.config.loopCount`, `:248`), `onFail` (`:249`).
- Chain `onFail` (snapshotted, reuses per-loop `OnFail`): `stop`→error halts (`:209-212`),
  `continue`→skip card + `hadFailure` (`:213-216`), `backoff`→end this pass, try next
  repetition (`:217-218`).
- `StackConfig` (`stack.ts:773-780`) carries `loopCount, scheduled, cron, guardrails, evals,
  defaults` — **no goal/acceptance field.** A stack goal would live here next to
  `loopCount`, be added to the launch snapshot (`stackRun.ts:246-250`), and gate the
  termination check (`stackRun.ts:149-160`). The purple **StackControlDock**
  (`web/src/lib/components/stacks/StackControlDock.svelte`) is the surface — it already
  hosts loop-count, schedule (stubbed), guardrails, evals ("chain acceptance"), and default
  config popovers; the goal is "one more facet."

---

## One-paragraph synthesis

lopi is materially further along than the roadmap states. A separate-model, maker/checker-
isolated, rubric-graded judge (**the Konjo Verifier**) is built, wired, persisted, and
already routes its critique into the next attempt — so A1 is not "build a judge" but
"promote the judge from a finalize-gate double-check into a first-class tiered eval
executor, checking an explicit goal/acceptance object, with the test scorer as the cheap
tier below it." The genuinely-missing pieces are: (a) an **eval/goal object** that actually
executes (the UI intent is inert), (b) closing the verifier's **fail-open** hole, (c) a
persisted **score-history query** for a live ratchet/no-progress gate (raw rows exist; the
query does not), (d) **budget metering** in the loop, and (e) a **stack-level goal** to turn
"advance N times" into "pursue an outcome." A2 and A3 are largely *productionizing and
measuring* primitives that already exist in skeleton (self-prompt reflection, fix-hint
routing, no-progress streak) rather than building from zero.
