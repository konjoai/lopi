# lopi — Competitive Roadmap (2026)

> Informed by a competitive survey of OpenClaw and the AI coding-agent
> orchestrator market (Devin, Conductor, Factory.ai, ccswarm, Sculptor,
> vibe-kanban, Cursor/Jules/Copilot background agents, OpenHands, Amp). See
> "Sources" at the bottom.

## TL;DR positioning

**OpenClaw** (formerly Clawdbot/Moltbot) is a *general-purpose, local-first
personal-automation agent* — a long-running process wired to messaging channels
(WhatsApp/Telegram/Slack/…) that runs shell/browser/cron tasks. It is **not** a
coding orchestrator: no deep codebase context, no git isolation, no CI loop.
lopi borrows OpenClaw's *control-UI* patterns but actually competes in the
**coding-orchestrator** lane against Devin, Conductor, Factory.ai Droids,
ccswarm, and the cloud background agents (Cursor/Jules/Copilot).

## What lopi already wins on (keep + market these)

| Capability | Status | Why it matters |
|---|---|---|
| Plan→Implement→Test→**Score**→Retry→PR loop | ✅ | The explicit *Score* step is rare in OSS — a real quality signal |
| Triple-surface dashboards (TUI + web + **macOS native**) | ✅ | Unique; Devin/Conductor/ccswarm each ship only one surface |
| GitHub **CI-failure → task injection** webhook | ✅ | Closes a loop most UI-layer orchestrators lack |
| **WhatsApp** + Telegram remote control | ✅ | WhatsApp reaches non-technical stakeholders; rare |
| All-**Rust** + Tokio orchestrator | ✅ | Resource predictability + security posture (cf. ZeroClaw demand) |
| Git branch isolation, parallel pool, SQLite memory, cron | ✅ | Table-stakes — covered |

## Highest-value gaps → new phases

Ranked by impact × differentiation × feasibility on lopi's existing architecture.

### Phase 10 — Cost & Budget Governance  ⭐ flagship
*The market's loudest pain (Uber's runaway agent bill); no OSS orchestrator
ships hard enforcement. lopi already emits budget events + has `lopi-ratelimit`.*
- Per-task and per-fleet **token/$ budgets** with hard circuit-breakers that
  auto-pause an agent at the ceiling (not just provider caps).
- Pre-emptive alerts at soft thresholds; live burn-rate + projection.
- **UI:** a Budget view (fleet + per-task meters, burn-down, history), per-pane
  spend, and a global "kill switch". *(web + macOS)*
- Crates: extend `lopi-ratelimit`, `lopi-core::Budget`, surface in `lopi-ui`.

### Phase 11 — Plan / Spec Approval Gate
*Jules, Devin, Copilot all gate execution behind a reviewed plan — cuts wasted
compute on wrong-direction runs.*
- Agent emits a structured **plan** (steps + files + estimated effort) and pauses.
- Human **approve / edit / reject** before Implement starts (configurable
  auto-approve for trusted repos).
- **UI:** a plan card in the pane with diff-of-intent + Approve/Reject. *(web + macOS)*

### Phase 12 — Fleet Command Center (Kanban)  ⭐ UI flagship
*Devin's Agent Command Center / vibe-kanban — the canonical fleet view. Pure UI
over state lopi already has.*
- Board of agents as cards in columns by phase
  (Queued · Planning · Implementing · Testing · Review · Done · Failed).
- Each card: goal, branch, **score**, **spend**, elapsed, attempt, PR link, CI badge.
- Drag to re-prioritize; click to focus the Forge pane. *(web first, then macOS)*

### Phase 13 — MCP Integration Layer
*Now table-stakes (ZeroClaw, Hermes, ccswarm, Composio all speak MCP). Unlocks
the 1,000+ Composio tool catalog + any user MCP server.*
- lopi as MCP **client** (agents gain MCP tools) and an MCP **server**
  (drive lopi from other agents).
- **UI:** an MCP servers panel (add/health/tool list). *(Config view)*

### Phase 14 — Specialist Agent Roles
*Factory's role-bounded Droids + ccswarm's Sangha consensus produce more
predictable, cheaper output than generalist fan-out.*
- Named roles with enforced scopes: **Planner · Coder · Reviewer · Verifier**.
- A **Reviewer/Verifier** agent runs adversarial review *before* the PR
  (severity-graded, security pass), feeding the Score.
- **UI:** role chips on panes + a review verdict panel.

### Phase 15 — Issue-Tracker as Task Queue + Review→Re-prompt Loop
*Emdash/Baton/Factory ingest from GitHub Issues/Linear; Conductor turns inline
diff comments into agent re-prompts.*
- GitHub **Issues / Linear** as first-class task ingestion (label → queue).
- **Inline diff comments → agent re-prompt** (bidirectional review). *(web)*

### Phase 16 — Loop Engineering  ⭐ flagship
*The discipline Boris Cherny & Peter Steinberger named: stop prompting agents
turn-by-turn, design the **loop** that prompts them. lopi already owns most of
the machinery (scorer, verifier, lessons, patterns, cron, plan gate, budgets) —
this phase makes it a **first-class, configurable, observable feature** with its
own sidebar screen. Full design: [`docs/LOOP_ENGINEERING.md`](LOOP_ENGINEERING.md).*

**Direction: ship Option E first, build toward B/C.** A `LoopConfig` TOML is the
source of truth (loop-as-code, git-tracked, PR-reviewed); the UI reads it and
adds the highest-value writable control (phased autonomy) + loop-health metrics.

**The three prongs (feature anatomy):**
1. *Context / instructions* — CLAUDE.md, VISION.md/AGENTS.md, per-repo `.lopi.toml`,
   skills library, custom commands, subagent defs, lessons DB, pattern store,
   just-in-time retrieval, compaction policy, external state file.
2. *Guardrails / standards / ethics / procedures* — permission allow/blocklists,
   auto-mode classifier, sandboxing, Stop + PostToolUse hooks, the four Konjo
   walls, harm boundaries, budget circuit-breakers, audit trail, **no-progress
   detector**, iteration hard cap.
3. *Direction / quality / gates* — cron, `/goal` conditions, CI-failure injection,
   issue ingestion, plan gate, **maker/checker split**, specialist roles, scoring,
   retry+backoff, stability pre-flight, postmortem, pattern enrichment, multi-loop
   coordination, **phased rollout levels (L1–L4)**, fleet command center.

**Options (all catalogued so nothing is lost):**

| Opt | Name | Scope | Effort | What it adds |
|-----|------|-------|--------|--------------|
| A | Read-only Loop Lens | Minimal | ~1 sprint | One screen surfacing every lever (CLAUDE.md, skills, rules, schedules, lessons, gates) — read only. |
| B | Control Panel | Moderate | 3–5 sprints | Full read/write of instructions + skills + guardrails + loop-health dashboard + per-schedule autonomy. |
| C | Self-evolving system | Ambitious | 8–12 sprints | B + **skill promotion** (lesson→skill), **no-progress detection**, **worktrees**, true **maker/checker**, **intent propagation**, **recipe library**. |
| D | Config-driven harness | Moderate | 3–4 sprints | Rich `.lopi/loop.toml` + `lopi loop validate`/`preview` CLI; minimal UI. Loop = code. |
| **E** | **Dual-track (chosen)** | Pragmatic | 4–6 sprints | D's `LoopConfig` truth + A's read-only UI + **phased autonomy picker** + loop-health metrics. Builds toward B then C. |

**Identified gaps this phase closes:** unified Loop-Eng surface · skill/instruction
management UI · **phased autonomy (L1–L4)** · loop-health/comprehension-debt
dashboard · VISION.md anchor + propagation · self-evolving skill capture ·
no-progress detector · git-worktree mode · true maker/checker split · per-loop
token economics.

**Build sequence (Option E → B → C), both surfaces in lockstep:**
- **16.1** — `LoopConfig` schema in `lopi-core`; `AutonomyLevel` enum on
  `ScheduleEntry`; `lopi loop validate` CLI; runner enforces levels
  (L1 report-only · L2 draft PR + human approve · L3 verifier-before-PR ·
  L4 auto-merge if verifier passes & score > threshold).
- **16.2** — `GET /api/loop-engineering` aggregation; `LoopHealthStore` in
  lopi-memory; **Loop** sidebar screen (read-only accordions) + Trust-Level
  dropdown per schedule. *(web + macOS in lockstep)*
- **16.3** — Loop-Health tab: 7-day sparklines (lessons, verifier pass rate,
  score trend); config-validity badge; per-schedule budget estimate;
  no-progress stall detection (`AgentEvent::ProgressStall`).
- **16.4 → B** — skill management UI (read + enable/disable); VISION.md field;
  maker/checker split (fresh-subprocess verifier).
- **16.5 → C** — skill promotion (lesson→skill suggestions), worktree mode,
  recipe library (Daily Triage · PR Babysitter · CI Sweeper · …), intent propagation.

**Key files:** `lopi-core/src/config.rs` (`LoopConfig`, `AutonomyLevel`,
`ScheduleEntry.autonomy_level`) · `lopi-agent/src/runner/run_loop.rs`
(no-progress detector, autonomy branch points) ·
`lopi-agent/src/runner/verifier_runner.rs` (maker/checker) ·
`lopi-memory/src/store/{lessons,schedules}.rs` (`LoopHealthStore`, autonomy
column) · `web/src/routes/loop/` (new route) · macOS new `Loop` NavSection + `LoopView`.

## Deferred / opportunistic
- **Git worktree mode** alongside branch isolation (disk efficiency on big repos). *(folded into Phase 16.5)*
- **Audit trail with replay/rollback** (ccswarm-style NDJSON per-step event log).
- **Merge-conflict prediction** across parallel agents (Clash-style pre-merge collision detection).
- **Self-evolving skills** (Hermes-style: capture successful task patterns as reusable prompt templates / CLAUDE.md snippets). *(folded into Phase 16.5)*
- **Slack @-mention** remote trigger (complements WhatsApp/Telegram).

## Build order (UI-forward, since dashboards are lopi's edge)
1. **Phase 12 — Command Center** (pure UI, immediate value, demoable).
2. **Phase 10 — Cost governance UI** (budgets already partly modeled).
3. **Phase 11 — Plan approval gate** (needs a backend plan event + approve endpoint).
4. **Phase 16 — Loop Engineering** (Option E: `LoopConfig` + autonomy + loop-health; the flagship feature).
5. **Phase 13–15** as backend capacity allows.

## Sources
Competitive survey (Jun 2026): Composio, Vellum, BuildBetter, Cubitrek,
Emergent, SparkCo (ZeroClaw), MindStudio, Milvus (OpenClaw explainer);
product docs for Conductor, Devin 2.0, Factory.ai Droids, Sculptor (Imbue),
vibe-kanban, Terragon, Cursor/Jules/GitHub Copilot agents, OpenHands,
SWE-agent, Sourcegraph Amp, ccswarm; TechCrunch (AI cost crisis).
