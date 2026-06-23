# Loop Engineering for lopi

> Design doc. Research synthesis + feature options for a production-grade
> "Loop Engineering" capability with its own sidebar screen.

## 1. What loop engineering is

Loop engineering is the practice of **removing yourself as the thing that prompts
the agent turn-by-turn**, and instead designing the autonomous *loop* that
discovers work, hands it to agents, verifies the result, persists state, and
decides the next action — on a schedule or until a measurable goal is met.

The discipline nests four layers, each wrapping (not replacing) the one below:

1. **Prompt engineering** (2022–24) — word choice, task framing.
2. **Context engineering** (2025) — curating everything the model sees at inference.
3. **Harness engineering** (2026) — tools, constraints, lifecycle hooks, feedback.
4. **Loop engineering** (2026) — the iterative control structure that drives the
   agent toward a goal autonomously.

> *"You shouldn't be prompting coding agents anymore. You should be designing
> loops that prompt your agents."* — Peter Steinberger
>
> *"I don't prompt Claude anymore. I have loops running that prompt Claude. My
> job is to write loops."* — Boris Cherny

### The two practitioners' rules

**Boris Cherny (Head of Claude Code):**
- **Verify first** — "give Claude a way to verify its work — that 2–3×'s the
  quality." The verifier closes the loop so you can walk away.
- **Write it down** — when Claude errs, document the fix in CLAUDE.md or a skill
  rather than correcting conversationally. Error reduction compounds.
- **Skills as institutional memory** — if done >1×/day, make it a skill.
- **Delegate, don't guide** — full brief up front (goal, constraints, acceptance
  criteria), launch, return when done or when a real question is asked.
- Runs 5 worktrees + 5–10 cloud sessions; `/loop`, `/schedule`, `/goal` primitives.

**Peter Steinberger (OpenClaw):**
1. Stop being the thing in the loop — write the loop once.
2. **Anchor intent** — VISION.md / CLAUDE.md / AGENTS.md so each tick knows where it's going.
3. **Give it something that says no** — tests, type checks, review gates = the loop's truth oracle.
4. **Give it skills worth calling** — named recipes, not one-off prompts.
5. **Cap it so it halts** — iteration limits, no-progress detection, dollar budgets.

**Supporting principles** (Addy Osmani, Cobus Greyling, Anthropic):
- "A decent model with a great harness beats a great model with a bad harness."
- **The Ratchet** — every line in your instructions should trace to a specific past failure.
- **Maker/checker split** — the agent that wrote the code must not be the agent that grades it.
- **Phased rollout** — L1 report-only → L2 assisted → L3 unattended-with-verifier → L4 auto-merge.
- **Deterministic checks beat model judgment** as feedback signals.
- **Context rot** — performance degrades as the window fills; load CLAUDE.md up front, grep the rest just-in-time.

## 2. The anatomy of a production loop

Grouped by the three prongs.

### Prong 1 — Context / instructions
CLAUDE.md · CLAUDE.local.md · VISION.md/AGENTS.md · per-repo `.lopi.toml` ·
skills library · custom slash commands · subagent definitions · lessons DB ·
pattern store · just-in-time retrieval · context compaction policy · external
state file (the Ralph-loop mechanism).

### Prong 2 — Guardrails / standards / ethics / procedures
Permission allow/blocklists · auto-mode classifier · sandboxing · Stop hooks ·
PostToolUse hooks · the four Konjo walls · ethics/harm boundaries · budget
circuit-breakers · audit trail · **no-progress detector** · iteration hard cap.

### Prong 3 — Scheduling / direction / quality / gates
Cron scheduling · `/goal` conditions · CI-failure injection · issue-tracker
ingestion · plan approval gate · **maker/checker split** · specialist roles ·
scoring · retry+backoff · stability pre-flight · postmortem · pattern
enrichment · multi-loop coordination · **phased rollout levels** · fleet command center.

## 3. What lopi already has vs. gaps

**Already shipped** (mapped to the anatomy): RepoProfile/.lopi.toml, pattern
store, lessons store, CLAUDE.md+skills+rules, researcher subagent, the Konjo
four walls, rubrics, forbidden-dirs, budget governance (P10), KV-cache eviction,
verifier, stability pre-flight, cron scheduling, CI webhook injection, plan
approval gate (P11), scorer, postmortem, pattern enricher, DAG task graph,
priority queue, agent pool, dead-letter queue, checkpoints, result cache,
remote control, triple-surface dashboards.

**Gaps (what "Loop Engineering" as a *feature* lacks):**
1. No unified Loop-Engineering surface — levers are scattered across files + TOML + pages.
2. No skill/instruction management UI (+ no per-skill activation history).
3. **No phased rollout / autonomy levels** (L1–L4) — every task runs at the same trust.
4. No loop-health / comprehension-debt dashboard.
5. No VISION.md intent anchor + propagation.
6. No self-evolving skill capture (lesson → named skill).
7. **No no-progress detector** (semantic stall).
8. No git-worktree mode (branch-only today).
9. Maker/checker split not truly wired (verifier shares the implementer's session).
10. No per-loop token economics (cost/tick, cumulative spend, burn projection).

## 4. Feature options

| Opt | Name | Scope | Effort | Core idea |
|-----|------|-------|--------|-----------|
| A | Read-only Loop Lens | Minimal | ~1 sprint | One screen surfacing CLAUDE.md, skills, rules, schedules, lessons, gates — read only. |
| B | Control Panel | Moderate | 3–5 sprints | Full read/write of instructions + skills + guardrails + a loop-health dashboard + per-schedule autonomy. |
| C | Self-evolving system | Ambitious | 8–12 sprints | B + skill promotion, no-progress detection, worktrees, true maker/checker, intent propagation, recipe library. |
| D | Config-driven harness | Moderate | 3–4 sprints | Rich `.lopi/loop.toml` + `lopi loop validate`/`preview` CLI; minimal UI. Loop = code. |
| E | **Dual-track (recommended)** | Pragmatic | 4–6 sprints | D's `LoopConfig` schema as the truth + A's read-only UI + **phased autonomy picker** + loop-health metrics. Builds toward B. |

## 5. Recommendation — Option E, building toward B

The most critical gaps are **phased autonomy levels** and **loop-health
observability** — the two things that separate a production loop from an
experiment, and exactly what Boris/Steinberger reduce to: *confidence controls*
+ *visibility*. A `LoopConfig` TOML makes loop engineering a first-class,
git-tracked, PR-reviewed artifact — the CLAUDE.md-as-team-asset practice applied
to the whole loop.

### Build sequence
- **Sprint 1** — `LoopConfig` schema in `lopi-core`; `AutonomyLevel` enum on
  `ScheduleEntry`; `lopi loop validate` CLI; enforce levels in the runner
  (L1 report-only · L2 draft PR + human approve · L3 verifier-before-PR ·
  L4 auto-merge if verifier passes & score > threshold).
- **Sprint 2** — `GET /api/loop-engineering` aggregation; `LoopHealthStore` in
  lopi-memory; Loop-Engineering sidebar screen (read-only accordions) + Trust
  Level dropdown per schedule (the one writable control).
- **Sprint 3** — Loop-Health tab: 7-day sparklines (lessons, verifier pass rate,
  score trend); config-validity badge; per-schedule budget estimate;
  no-progress stall detection (`AgentEvent::ProgressStall`).
- **Sprint 4** (toward B) — skill management UI (read + enable/disable);
  VISION.md field; maker/checker split (fresh subprocess verifier).

### Key files
- `crates/lopi-core/src/config.rs` — `LoopConfig` + `AutonomyLevel`; `ScheduleEntry.autonomy_level`.
- `crates/lopi-agent/src/runner/run_loop.rs` — no-progress detector, autonomy branch points.
- `crates/lopi-agent/src/runner/verifier_runner.rs` — maker/checker split.
- `crates/lopi-memory/src/store/{lessons,schedules}.rs` — `LoopHealthStore`, autonomy column.
- `web/src/routes/loop/` — new route beside `budget/`, `schedules/`, `config/`.
- macOS: new `Loop` `NavSection` + `LoopView` mirroring the web screen.

## 6. Self-Prompting Strategy Engine (Phase 16.4 — shipped)

The single highest-leverage lever in any retry loop is the **self-prompt**: the
text the agent feeds back into its *own* next planning step after a failed
attempt. A raw error dump is one strategy among many; reframing the failure into
a structured self-reflection lifts retry success substantially on coding tasks
(Reflexion, +17 pp on HumanEval). lopi makes this a first-class, pickable,
loop-as-code lever.

### The S1–S4 ladder

`SelfPromptStrategy` (`crates/lopi-core/src/self_prompt.rs`) is a pure transform
`frame(base_failure, attempt) -> String`. Ordered by how much cognitive
scaffolding each adds before the agent re-plans:

| Tag | Strategy | What it injects into the next prompt | Provenance |
|-----|----------|--------------------------------------|------------|
| **S1** | Direct | The raw failure, verbatim (legacy default — byte-identical). | baseline |
| **S2** | Reflexion | "Name the single root cause, then try a *different* approach." | Shinn et al. 2023 ([2303.11366](https://arxiv.org/abs/2303.11366)) |
| **S3** | Self-Refine | "Critique against correctness/coverage/minimality, then revise each bullet." | Madaan et al. 2023 ([2303.17651](https://arxiv.org/abs/2303.17651)) |
| **S4** | Plan-Then-Act | "Produce a numbered, dependency-ordered plan before editing a single line." | Wang et al. 2023 (Plan-and-Solve) |

`Direct` reproduces the legacy raw-failure injection exactly, so the default is a
no-op change; richer strategies prepend a self-prompting preamble + concrete
instruction.

### Full-stack wiring

- **Core** — `LoopConfig.self_prompt` field (loop-as-code); `LoopConfig::save_to_repo`
  writes `.lopi/loop.toml` so the UI can persist the choice.
- **Runner** — `AgentRunner::with_self_prompt(strategy)`; the adaptive-retry path
  routes the failure block through `strategy.frame(..)` before injecting it into
  the next planning prompt. Wired live from `.lopi/loop.toml` in both the
  `lopi run` CLI path and the orchestrator pool.
- **API** — `GET /api/loop-engineering` carries a `self_prompt_strategies` catalog
  (each with a live self-prompt **preview**); `POST /api/loop-engineering/strategy`
  validates + persists the choice (`422` on unknown tags).
- **Web + macOS** — a "Self-Prompting Strategy" panel: picker, strategy cards, and
  a live preview of the exact self-prompt the agent will generate.

### Next two (research-ranked, not yet built)

The [discovery sweep](#sources) ranked these as the highest-value follow-ons:

2. ~~**Adaptive Strategy Escalation**~~ — ✅ **shipped (Phase 16.5)**: auto-climb
   S1→S4 by attempt number instead of pinning one strategy. `LoopConfig.escalate_strategy`
   + `SelfPromptStrategy::escalated(base, attempt)` (climb one rung per failed
   attempt, capped at S4, starting from the base). Wired into the runner
   (`AgentRunner::effective_strategy`), surfaced as an escalation-ladder preview
   and a toggle in the web + macOS Loop screens, persisted via
   `POST /api/loop-engineering/escalation`. Backed by RefineCoder
   ([2502.09183](https://arxiv.org/abs/2502.09183)).
3. ~~**Earned-Trust Auto-Promotion**~~ — ✅ **shipped (Phase 16.7)**: a repo or
   schedule earns one rung up the L1→L4 ladder after N consecutive clean,
   verifier-passed runs and is demoted on a post-merge revert. Pure
   `EarnedTrust` state machine (`on_clean_run` / `on_failed_run` / `on_revert`)
   + `AutonomyLevel::{promoted,demoted}` + `LoopConfig.{promote_after,trust_ceiling}`
   loop-as-code levers, persisted in a `trust_ledger` table
   (`record_clean_run` / `record_failed_run` / `record_revert`). Backed by the
   CSA Agentic Trust Framework (2026). Live recording wiring, GitHub revert
   detection, and the Loop-screen surface are the follow-on.

Critical safety adjacency: ~~wire `LoopConfig.budget_tokens` to the Claude API
`task_budget` parameter~~ — ✅ **shipped (Phase 16.6)**: `LoopConfig.budget_tokens`
is forwarded to the Anthropic `task_budget` output config (beta
`task-budgets-2026-03-13`) on the direct-API planning path so the model
self-regulates instead of being hard-cut by `max_tokens`. The decision logic
lives in pure helpers (`api_budget::{supports_task_budget, effective_task_budget,
task_budget_output_config}`): the budget is **model-gated** (only Opus 4.7/4.8 and
Fable 5 accept the parameter — it is silently dropped on the Haiku/Sonnet tiers
lopi uses for cheap early attempts) and **clamped** up to the API's 20,000-token
minimum so an under-minimum config never 400s. Wired through
`AgentRunner::with_task_budget` from `.lopi/loop.toml` in both the `lopi run` CLI
path and the orchestrator pool.

## Sources
Reflexion ([2303.11366](https://arxiv.org/abs/2303.11366)) · Self-Refine
([2303.17651](https://arxiv.org/abs/2303.17651)) · SELF-DISCOVER
([2402.03620](https://arxiv.org/abs/2402.03620)) · LATS
([2310.04406](https://arxiv.org/abs/2310.04406)) · RefineCoder
([2502.09183](https://arxiv.org/abs/2502.09183)) · Iterative Self-Repair
([2604.10508](https://arxiv.org/abs/2604.10508)) · Coding-Agent Scaffold Taxonomy
([2604.03515](https://arxiv.org/abs/2604.03515)) ·
howborisusesclaudecode.com · theneuron.ai (Cherny/Wu) · cobusgreyling
substack/medium · addyosmani.com (loop + harness engineering) · tosea.ai ·
steipete.me (just-talk-to-it, optimal-ai-dev-workflow) · anthropic.com
engineering (context engineering, long-running harnesses, agent skills, best
practices) · github.com/cobusgreyling/loop-engineering ·
github.com/ai-boost/awesome-harness-engineering.
