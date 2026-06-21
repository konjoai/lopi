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

## Sources
howborisusesclaudecode.com · theneuron.ai (Cherny/Wu) · cobusgreyling
substack/medium · addyosmani.com (loop + harness engineering) · tosea.ai ·
steipete.me (just-talk-to-it, optimal-ai-dev-workflow) · anthropic.com
engineering (context engineering, long-running harnesses, agent skills, best
practices) · github.com/cobusgreyling/loop-engineering ·
github.com/ai-boost/awesome-harness-engineering.
