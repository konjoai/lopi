# The lopi Loop Framework · v2

**Status:** v2, decisions locked · **Scope:** lopi orchestrator, all surfaces (CLI, Telegram, web UI)
**Companion visual:** `loop-framework-visual.html`

**Changes from v1:** verb alias pack (:research, :scope, :plan, :optimize, :benchmark), plan mutation methods, `budget:auto` window-aware guard, cross-repo chains promoted to v1 option, four decisions resolved, naming candidates.

---

## 1. The problem

Three things keep happening in agent-driven work:

1. **Continuation.** A session ends mid-task. The next session should pick up where the last one left off without a human reconstructing context.
2. **Grounding.** "Make progress" is vibes unless the agent knows exactly which files define progress: PLAN.md, PROGRESS.md, logs, git state.
3. **Repetition.** Real work is iterative. Run until tests pass. Run three times. Run every night at 2am. Run tomorrow at 2pm, twice.

The loop framework makes these one first-class object instead of hand-written continuation prompts, external cron jobs, and retry logic buried in code.

## 2. The mental model

> **A loop is a prompt that runs again, with a contract for what it reads and writes, a trigger for when, and guards for when to stop.**

Everything in the framework is one of four primitives or one of two composition operators. If a feature can't be expressed in those six things, it doesn't go in.

| Primitive | Question it answers | Examples |
|---|---|---|
| **Prompt** | What runs? | `"fix the failing test"`, `:progress`, `:kiban-sprint` |
| **Contract** | What does each iteration read and write? | reads PLAN.md + PROGRESS.md + logs, writes PROGRESS.md + NEXT.md |
| **Trigger** | When do iterations start? | now, `at:"0 14 * * *"`, `every:15m`, `on:"ci-fail"` (v1.1) |
| **Guards** | When do they stop? | `x3`, `until:"cargo test"`, `max:25`, `budget:auto`, `window:8h` |

| Operator | Meaning | Example |
|---|---|---|
| **Stack** (separate sends) | Run independently, in queue order | prompt A, then prompt B |
| **Chain** (`>`) | Each step's output feeds the next step's context | `:plan > :implement > :test` |

A loop can wrap a single prompt or a whole chain. That one rule produces every advanced pattern without new concepts.

## 3. Prompts: three kinds

**General prompts** carry no task detail because the contract supplies it. The canonical one is `:progress`: read the contract files, identify the next incomplete item, do it, record the outcome.

**Specific prompts** are literal instructions: `"Add INT3 fused dequant path for Qwen2.5-7B"`.

**Aliases** are named prompts from a library, invoked with a leading colon. kiban ships the default pack; repos and users extend it in `loops.toml`.

### 3.1 The default alias pack

Each alias has a **write class** that bounds what it may touch. This is the safety taxonomy: a `read` alias physically cannot commit code, and mixing classes in one alias is not allowed.

| Alias | Write class | What it does |
|---|---|---|
| `:research` | read | Investigate a question against code, docs, logs, or the web. No code changes. Writes findings to `RESEARCH.md` and appends a summary to PROGRESS.md. Takes an argument: `:research "MLX paged attention approaches"`. |
| `:plan` | plan | Create or reconcile PLAN.md: fold PROGRESS.md outcomes back in, reorder by priority, mark done items, surface blockers. The plan's maintenance loop. |
| `:scope` | plan | Take one PLAN.md item and bound it kiban-style: explicit non-goals, phases with per-phase verify, and a pre-registered kill-test gate definition. Output is a sprint spec appended to PLAN.md. `:scope "INT3 dequant"`. |
| `:goal` | plan | Add, amend, or retire a top-level goal. `:goal "ship INT3 for the full Qwen2.5 family"`. Goals live at the top of PLAN.md; `:plan` reconciles beneath them. |
| `:progress` | code | The general continuation prompt. Next incomplete item, do it, record it. |
| `:implement` | code | Like `:progress` but scoped to the current sprint spec only. Refuses to wander outside the scoped phases. |
| `:optimize` | code | Optimization pass under full kiban discipline. **Requires `gate:`** and refuses to start without it. Merges only on measured improvement: 30-run paired Wilcoxon, p<0.05, significance and effect size. Otherwise reverts and records the revert as a finding. |
| `:benchmark` | read | Run the bench harness: 30 runs, paired stats, CoV gates, results written to `benchmarks/` with hardware, model format, and configuration scoped on every number. Measures only, never merges. |
| `:kill-test` | read | Run the pre-registered measurement gate only, report pass/fail. |
| `:tidy` | code | Konjo quality pass: lint, dead code, doc-drift check against live source. |
| `:handoff` | plan | Write NEXT.md summarizing state for the next session. No code changes. |
| `:kiban-sprint` | code | Full sprint: `gate:` pre-flight, scoped phases with verify, post-flight ledger + CHANGELOG + NEXT.md + VERSION. |

The classes also compose into readable pipelines: `:research > :scope > :implement > :benchmark` reads, then plans, then builds, then measures, and each step's class tells you what it could possibly have changed.

### 3.2 Alias arguments

A quoted string immediately following an alias (no `>` between them) binds as its argument:

```
:research "prefix-reuse eviction strategies"
:scope "INT3 full-family support"
:goal "cold-start under 200ms on 16 GB"
```

Grammar rule: quoted text after `>` or at the start of a spec is a literal prompt step; quoted text directly after an alias is that alias's argument.

## 4. The context contract

Each iteration:

**Reads (before):** `PLAN.md` (goals + intended work), `PROGRESS.md` (append-only outcome log), `NEXT.md` (previous handoff), recent run logs and git status.

**Writes (after):** one appended PROGRESS.md entry (what was attempted, what happened, evidence), a rewritten NEXT.md, and a ledger row in SQLite (loop id, iteration, outcome, cost, repo).

Defaults are those filenames at repo root; `loops.toml` remaps per repo. **The write half is enforced: an iteration that records nothing is a failed iteration**, because an unrecorded iteration breaks continuation for every future session. Negative results are written the same as wins, so tomorrow's `:progress` knows the speculative path was tried and reverted.

### 4.1 Updating goals, plans, and sprints

Three methods, in increasing automation:

1. **Direct edit.** PLAN.md and friends are plain files. Edit them in your editor; the next iteration reads the new state. No framework ceremony required, ever.
2. **Mutation aliases.** `:goal`, `:plan`, `:scope`, `:handoff` are the structured path: they parse the existing files, apply the change, and keep formatting consistent so `:progress` can always parse its own plan. `:plan` is the reconciler; running it periodically (`:plan every:24h max:1`) keeps PLAN.md honest against PROGRESS.md drift.
3. **UI (later).** A plan panel in the web pane editing the same files. Nothing special: the files remain the single source of truth, so all three methods stay consistent by construction.

## 5. Triggers

- **now** (default): enters the queue immediately.
- **at:"cron"** / **every:duration**: when the trigger fires, the loop enters the queue. It does not preempt a running agent unless marked priority.
- **on:"event"** (v1.1, deferred): `ci-fail`, `file-change:<glob>`, `queue-empty`.

## 6. Guards

| Guard | Meaning | Default |
|---|---|---|
| `xN` / `x∞` | iteration count | `x1` |
| `until:"cmd"` | stop when the command exits 0, checked after each iteration | unset |
| `gate:"cmd"` | pre-flight: must exit 0 before iteration 1, else the loop refuses to start | unset |
| `max:N` | hard ceiling regardless of `until` | 25 |
| `budget:` | token guard: `auto`, explicit `200k`, or `none` | **auto** |
| `window:T` | wall-clock ceiling | 8h |
| `on-fail:` | `stop` \| `continue` \| `backoff` | `stop` |

**Decision (locked): `x∞` stays, with visible implicit guards.** Enabling loop defaults to ∞; `max`, `budget`, and `window` still apply and render dim on the card. Truly unbounded requires explicit `max:none`, a deliberate act.

**Conditions are shell exit codes.** `until:"cargo test"` is the whole condition language. No DSL.

### 6.1 `budget:auto` — the window-aware token guard

lopi runs on subscription credentials, which means every iteration spends from the same pool as interactive use: the rolling 5-hour session window and the weekly cap. `budget:auto` makes loops live inside those limits instead of blowing through them at 3am.

**What lopi can and cannot see.** Anthropic exposes no remaining-limit API for subscriptions. lopi self-meters: every iteration's token usage is already in the ledger (Claude Code reports usage in its stream-json results). lopi therefore knows its own spend precisely and the human's interactive spend not at all. Two consequences fall out honestly:

1. **Capacity is learned, not known.** Window capacity starts from a conservative configured estimate. When a run hits an actual limit error, lopi records the observed ceiling, tightens its estimate, and parks affected loops until the window rolls. Over a week of use the estimate converges. Observed limit hits are findings, logged like everything else.
2. **A reserve is structural, not polite.** Because interactive usage is invisible to lopi, `budget:auto` never plans to consume the full estimated window. Default reserve: 30% of the window, 20% of the weekly cap, configurable.

**The check, at every iteration boundary:**

```
estimate = EMA of recent iteration costs (per alias, per repo)
window_spend = ledger sum over trailing 5h
weekly_spend = ledger sum over trailing 7d

proceed iff  window_spend + estimate ≤ window_capacity × (1 − window_reserve)
        and  weekly_spend + estimate ≤ weekly_capacity × (1 − weekly_reserve)
```

**Defer, don't fail.** A loop blocked by budget enters a `waiting · budget` state with a computed resume time (oldest spend in the trailing window + 5h). The card shows it in dim cyan: "waiting · budget · resumes ~14:20". Cron loops that would fire into an exhausted window defer the same way; the fire is late, not lost, and the lateness is recorded.

**Pacing (optional).** For `x∞` and long scheduled loops, `pace:even` spreads the loop's budget share across the window instead of running iterations back-to-back, so an overnight loop can't exhaust the window in its first hour.

```toml
[budget]
mode            = "auto"
window_capacity = "1.2M"     # starting estimate, tightened by observed limit hits
weekly_capacity = "20M"
window_reserve  = 0.30
weekly_reserve  = 0.20
defer           = true
```

Explicit `budget:200k` still works as a per-loop ceiling and stacks with auto (both must pass). `budget:none` opts a loop out of auto, which, like `max:none`, is deliberate and visible.

## 7. Composition

**Stack** is the existing queue. **Chain** (`>`) passes artifacts forward: each step's contract writes plus a HANDOFF blob (the step's final summary) join the next step's reads.

**Wrapping:** guards and triggers attach to whatever they follow.

```
:implement x3                          loop a single prompt
(:plan > :implement > :test) x2        loop a whole chain twice
:research "RaBitQ" > :scope > :implement > :benchmark
:nightly at:"0 2 * * *"                schedule an alias that is itself a chain
```

**Decision (locked): chain steps share one PROGRESS.md stream with step tags.** Entries carry `[step::plan]`-style tags. One stream reads as a narrative for humans; the tags keep it parseable per step.

**Failure semantics:** a failing step consults its `on-fail`. `stop` kills the chain iteration; `continue` proceeds with the failure recorded in the handoff so the next step knows. A chain iteration counts toward `xN` only when it completes end to end.

### 7.1 Cross-repo chains (`@repo`) — v1 option

**Decision (locked): shipping as an option.** A `@repo` token on any unit overrides the directive's default repo for that step:

```
:benchmark @vectro > :report @konjo.ai
:tidy @squish > :tidy @vectro > :tidy @kiban
```

Semantics that keep it sane:

- Each step runs in a git-isolated worktree of **its own** repo and commits there. No shared branches across repos, ever.
- Contract files are per-repo: a step reads and writes the contract of the repo it runs in. The HANDOFF blob is what crosses the boundary, carrying the previous step's summary and named artifacts (file paths are copied into the next step's workspace, not referenced across worktrees).
- The ledger records repo per step, so a cross-repo chain's history is auditable per repository.
- `budget:auto` is global, not per-repo: the window is one pool regardless of where iterations run.

Constraint to respect in the implementation: this breaks the current one-repo-per-directive assumption, so directives gain a repo *per step* rather than per directive. That is the single schema change cross-repo requires.

## 8. Modifying a running loop

Iterations are atomic. Edits apply at iteration boundaries, never mid-iteration. Live controls: **pause / resume**, **drain** (finish current iteration, then dequeue), **kill** (abort now, record it), **bump** (`+N` more iterations).

## 9. The grammar

```
loopspec  := chain modifier*
chain     := unit ( ">" unit )*
unit      := ( ":" alias [quoted-arg] | quoted-text | "(" chain ")" ) [ "@" repo ]
modifier  := "x"N | "x∞"
           | until:"cmd" | gate:"cmd"
           | at:"cron"   | every:duration | on:"event"
           | max:N | budget:(auto|N|none) | window:duration | on-fail:policy | pace:even
```

Worked examples:

```
"fix the flaky websocket test"                       run once
:progress x3                                         loop the general prompt 3 times
:progress until:"cargo test" max:5                   loop until green, cap at 5
:kill-test at:"0 14 * * *" x2                        tomorrow 2pm, run twice
:research "paged attention" > :scope > :implement    verb pipeline
:benchmark @vectro > :report @konjo.ai               cross-repo chain
:optimize gate:"./kill_test.sh" until:"cargo test"   kiban-disciplined optimization
:progress x∞ pace:even                               overnight, paced within the window
```

One grammar, three surfaces: `lopi run '<spec>'`, a Telegram message, and the UI cards as a visual editor for the same string.

## 10. Configuration: `loops.toml`

```toml
[contract]
plan     = "PLAN.md"
progress = "PROGRESS.md"
next     = "NEXT.md"
research = "RESEARCH.md"

[defaults]
max     = 25
window  = "8h"
on_fail = "stop"

[budget]
mode            = "auto"
window_capacity = "1.2M"
weekly_capacity = "20M"
window_reserve  = 0.30
weekly_reserve  = 0.20
defer           = true

[alias.nightly]
spec = ':tidy > :kill-test at:"0 2 * * *" max:1'

[alias.launch-check]
spec = ':benchmark @squish > :research "Ollama upstream changes" > :handoff'
```

kiban ships the default alias pack (section 3.1) as a language pack; repos extend it. Aliases with a `spec` key are the "aliases all the way down" mechanism: a saved workflow is just a bigger alias.

## 11. Safety and honesty defaults

- Every iteration writes to the ledger, including failures, kills, budget deferrals, and observed limit hits.
- Implicit guards render dim on every card and in `lopi status`. Nothing invisible governs execution.
- `gate:` failures refuse to start and say why; they never retry silently.
- Write classes bound what each alias may touch; a `read` alias cannot commit.
- Chains record per-step outcomes, so `on-fail:continue` can't launder a failed step into a green run.

## 12. Escalation ladder

| Level | You learn | Example |
|---|---|---|
| L0 | send a prompt | `"fix the test"` |
| L1 | `xN` | `:progress x3` |
| L2 | `until:` (+ default guards) | `:progress until:"cargo test" max:5` |
| L3 | `at:` / `every:` | `:kill-test at:"0 14 * * *" x2` |
| L4 | `>` chains (and `@repo` when needed) | `:research "X" > :scope > :implement` |
| L5 | name it | `[alias.nightly]` → `:nightly` |

## 13. Decisions

**Resolved:**

1. ∞ stays as the loop-enable default, with implicit guards rendered visibly. `max:none` is the explicit escape hatch.
2. Chain steps share one PROGRESS.md stream with step tags.
3. Event triggers (`on:`) defer to v1.1. Cron and now cover launch use cases.
4. Cross-repo chains ship in v1 as an option via `@repo`, with per-step worktrees and copied artifacts.
5. `budget:auto` is the default budget guard, self-metered with learned capacity and structural reserves.

**Still open:**

1. Should `:optimize` also require `until:` or is `gate:` + Wilcoxon-on-merge enough?
2. `pace:even` in v1 or v1.1? (Defer check is v1 regardless; pacing is refinement.)
3. The name (section 14).

## 14. Naming

Candidates in the kiban tradition. Short, lowercase, Japanese, one honest idea each.

| Name | Kanji | Meaning | Why it fits | Caveat |
|---|---|---|---|---|
| **rasen** | 螺旋 | spiral, helix | The framework's whole thesis: the contract means each pass reads what the last wrote, so a loop that makes progress is a spiral, not a circle. Same shape, always higher. | Slightly abstract on first hearing |
| **junkan** | 循環 | circulation, cyclic flow | Loops as the circulatory system of the repos: continuous, life-sustaining, self-regulating (budget:auto as homeostasis). | Common word, less distinctive |
| **shūki** | 周期 | period, cycle | The scheduling half built into the name: periodic functions, cron, orbital periods. Pairs with the constellation aesthetic. | Romanization wobbles (shuki/shuuki) |
| **kaiten** | 回転 | rotation | Kaiten-zushi: prompts on a conveyor, each arriving at the agent in turn. Playful and exactly what the queue looks like. | The sushi reading may dominate |
| **junkai** | 巡回 | patrol rounds | The original candidate. An agent making its rounds: check, act, move on, return. | Patrol implies watching more than building |
| **meguri** | 巡り | cycle, pilgrimage rounds | Softer reading of the same root; a meguri is a deliberate circuit of meaningful stops. | Less technical register than siblings |
| **rinne** | 輪廻 | samsara, cycle of rebirth | Each iteration dies and is reborn knowing what came before (the contract as karma). Evocative. | Heavy religious connotation |
| **hanpuku** | 反復 | repetition, iteration | The literal translation of "iteration." Maximum honesty. | Flat; no second layer |
| **wa** | 輪 | ring, loop, wheel | Two characters of grammar: `:progress` is a wa. Homophone with 和 (harmony). | Maybe too short to search or brand |
| **kairo** | 回路 | circuit | Electrical circuit: loops with gates. Also homophone of 懐炉 (pocket warmer), a fire in your pocket, which suits the orb. | **One letter from your existing repo kairu. Collision risk.** |

Recommendation: **rasen**. It is the only candidate that names the difference between this framework and a bare while-loop. The contract is what turns repetition into ascent, and "spiral, not circle" is a one-line explanation anyone gets immediately. junkan and shūki are the strong alternates. Avoid kairo despite its charm; the kairu collision will bite in conversation and in search.

## 15. Implementation sketch (Rust side)

```rust
struct LoopSpec {
    chain:    Vec<Step>,
    count:    Count,                    // N | Infinite
    until:    Option<Cmd>,
    gate:     Option<Cmd>,
    trigger:  Trigger,                  // Now | Cron(Schedule)   (Event in v1.1)
    guards:   Guards,                   // max, budget, window, on_fail, pace
}
struct Step {
    unit:  Unit,                        // Alias { name, arg } | Literal(String) | Group(Vec<Step>)
    repo:  Option<RepoRef>,             // @repo override; None = directive default
}
struct BudgetMeter {                    // budget:auto
    window_capacity: TokenEst,          // learned; tightened on observed limit hits
    weekly_capacity: TokenEst,
    reserves: (f32, f32),
    // spend queried from ledger: trailing 5h / trailing 7d sums
}
```

- Parser: single-pass tokenizer, ~350 lines with `@repo` and alias args; property-test the round-trip (spec → string → spec).
- Scheduler: cron enqueues into the existing priority queue; one Task per iteration, so git isolation, retries, and the TUI work unchanged. Cross-repo steps resolve their own worktree at step start.
- Contract hooks: pre-hook assembles reads into prompt context; post-hook validates writes (missing PROGRESS entry ⇒ iteration failed) and applies step tags.
- BudgetMeter: ledger-backed trailing sums, EMA estimator keyed (alias, repo), limit-hit observer that tightens capacity and computes resume times.
- Ledger: extend MemoryStore with `loop_id`, `iteration`, `step`, `repo`, `outcome`, `cost`, `deferred_for`.

Build order, each stage shippable: parser → contract hooks → count/until guards → budget meter (defer only) → cron trigger → chains + step tags → `@repo` → aliases-with-spec → pace.
