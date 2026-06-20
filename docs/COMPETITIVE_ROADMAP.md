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

## Deferred / opportunistic
- **Git worktree mode** alongside branch isolation (disk efficiency on big repos).
- **Audit trail with replay/rollback** (ccswarm-style NDJSON per-step event log).
- **Merge-conflict prediction** across parallel agents (Clash-style pre-merge collision detection).
- **Self-evolving skills** (Hermes-style: capture successful task patterns as reusable prompt templates / CLAUDE.md snippets).
- **Slack @-mention** remote trigger (complements WhatsApp/Telegram).

## Build order (UI-forward, since dashboards are lopi's edge)
1. **Phase 12 — Command Center** (pure UI, immediate value, demoable).
2. **Phase 10 — Cost governance UI** (budgets already partly modeled).
3. **Phase 11 — Plan approval gate** (needs a backend plan event + approve endpoint).
4. **Phase 13–15** as backend capacity allows.

## Sources
Competitive survey (Jun 2026): Composio, Vellum, BuildBetter, Cubitrek,
Emergent, SparkCo (ZeroClaw), MindStudio, Milvus (OpenClaw explainer);
product docs for Conductor, Devin 2.0, Factory.ai Droids, Sculptor (Imbue),
vibe-kanban, Terragon, Cursor/Jules/GitHub Copilot agents, OpenHands,
SWE-agent, Sourcegraph Amp, ccswarm; TechCrunch (AI cost crisis).
