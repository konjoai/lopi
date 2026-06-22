# The Pentad — Loop Engineering Completion Roadmap

> **North star:** lopi is no longer a thing you *prompt*. It is a loop you *design*.
> This roadmap closes the five (+ one) building blocks of loop engineering to a
> production-grade, Konjo standard — every gate green, every line tracing to a
> failure, every loop able to run unattended and *halt on its own terms*.
>
> Companion design doc: [`LOOP_ENGINEERING.md`](./LOOP_ENGINEERING.md). That file
> argues *why* and proposes the `LoopConfig` surface; **this** file is the
> execution plan: gap matrix → movements → sprints, each with a Konjo Definition
> of Done.

---

## 0. The model we are completing

Addy Osmani's anatomy (named by Boris Cherny's "my job is to write loops"): a
useful loop needs **five building blocks and one memory layer**.

| # | Block | Canonical definition (Osmani) | Why it matters |
|---|-------|-------------------------------|----------------|
| 1 | **Automations** | "discovery + triage on a schedule / API call / git event" | The loop starts itself; you stop being the trigger. |
| 2 | **Worktrees** | isolated checkouts so "one agent's edits literally can not touch the other one's" | Real parallelism without collision. |
| 3 | **Skills** | "how you stop re-explaining the same project context every session" (`SKILL.md`) | Institutional memory the agent loads, not guesses. |
| 4 | **Plugins & connectors** | MCP + integrations so the loop "acts inside your actual environment" | The loop changes the world, not just a transcript. |
| 5 | **Sub-agents** | "splitting the one who writes from the one who checks" | The verifier closes the loop → 2–3× quality. |
| 6 | **Memory / external state** | "The agent forgets, the repo doesn't" — a file/board outside the conversation | Continuity across ticks; the Ralph mechanism. |

---

## 1. Current state — audited

Status from a full-tree audit (`crates/`, `src/`, `web/`, `macos/`, `.claude/`,
`.konjo/`). Legend: 🟢 solid · 🟡 partial · 🔴 missing.

| Block | Status | What exists | The true gap |
|-------|--------|-------------|--------------|
| **Automations** | 🟢 | `lopi-orchestrator` (`scheduler.rs`, `schedule_manager.rs`) cron; `lopi-webhook` CI-failure → task with HMAC verify; per-schedule autonomy L1–L4; run-history persistence | No dead-letter queue, no event **dedup**, synchronous triage, no event-payload **templating**, no change audit trail |
| **Worktrees** | 🟡 | `lopi-git/manager.rs` branch-per-attempt (`lopi/{task}-attempt-{n}`), `WORKTREE_LOCK`, per-repo semaphore, `hard_rollback`, `open_pr`/`auto_merge`, `CARGO_TARGET_DIR` split | **Branch-only — no real `git worktree`.** Concurrent runs share one working dir; the lock serializes what worktrees would parallelize. No mid-run snapshot, no rebase-on-moved-main, no branch GC |
| **Skills** | 🟡 | `.claude/skills/` (6 `SKILL.md`, web-only); `lopi-tools` `ToolRegistry`; pattern/lesson injection into the planning prompt | **No runtime skill engine** — skills never load into the lopi agent loop; no invocation, versioning, activation audit, or lesson→skill **promotion** |
| **Plugins & connectors** | 🟡 | `lopi-remote` (Telegram + WhatsApp), `lopi-webhook` (GitHub), `lopi-github` client | **No MCP at all** — neither consumes external MCP servers nor exposes lopi as one. Connectors are hardcoded singletons; no plugin loader |
| **Sub-agents** | 🟡 | `lopi-orchestrator` `AgentPool` (bounded concurrency), `constellation` routing, `q_router` (ε-greedy), capability matching | **Peer-level workers only** — no true sub-agent spawn, no **maker/checker split** (verifier shares the implementer's session), no decomposition/delegation/inter-agent messaging |
| **Memory / state** | 🟡 | `lopi-memory` SQLite (patterns, lessons, audit, schedules); `CLAUDE.md` + rules; `LoopConfig` → `.lopi/loop.toml` | No per-loop **external state file** (Ralph), no `VISION.md` intent anchor + propagation, no no-progress/semantic-stall detector |

**Verdict:** the *backbone* is production-grade; the *loop-defining* primitives
(true worktrees, MCP, runtime skills, maker/checker) are the missing 20% that
separate "an orchestrator with cron" from "a loop you can walk away from."

---

## 2. Principles this roadmap is held to (Konjo)

1. **The Ratchet** — every new constraint/skill traces to a specific past failure.
2. **Maker ≠ checker** — the agent that writes is never the agent that grades.
3. **Deterministic oracles first** — tests/types/scope-checks beat model judgment.
4. **Phased autonomy** — L1 report → L2 draft-PR → L3 verified-PR → L4 auto-merge; nothing ships above its earned trust.
5. **Cap it so it halts** — iteration limits, no-progress detection, dollar budgets are *required*, not optional.
6. **Loop-as-code** — every lever lives in `.lopi/loop.toml`, git-tracked and PR-reviewed.
7. **Three Walls on every sprint** — pre-commit hooks, CI gate (coverage ≥ 80% / target ≥ 95%, complexity ≤ 15, file ≤ 500 LOC, fn ≤ 50 LOC, zero undocumented public APIs, `audit`+`deny` clean), adversarial PR review.

---

## 3. The roadmap — Phase 17: "The Pentad"

Five **movements**, sequenced by dependency. Each movement closes one block to
🟢 and is independently shippable behind a feature flag.

```
M1 Worktrees ──────────┐   (unblocks true parallel sub-agents)
                       ├──> M4 Sub-agents (maker/checker) ──┐
M2 Skills ─────────────┤                                    ├──> M6 The Loop Surface
M3 Connectors / MCP ───┘   (unblocks skills-as-MCP + acting)│        (unify + observe)
                                                            │
M5 Automations hardening + Memory/state ────────────────────┘
```

**Why this order:** worktrees (M1) are the substrate real sub-agents (M4) stand
on; MCP (M3) is how both skills *act* and connectors *reach out*; skills (M2)
and MCP can run in parallel; automations/memory hardening (M5) is low-risk and
fills the cron path while the heavy work lands; the unifying surface (M6) comes
last so it reflects finished primitives, not moving ones.

**Estimated envelope:** ~18 sprints. A sprint = one PR-sized increment, every
wall green. Movements M1–M3 run partially in parallel across worktrees (dogfood).

---

## 4. Sprints

Each sprint lists **Goal · Deliverables · Key files · Konjo DoD**. DoD assumes
the standing Three-Wall gates; only sprint-specific acceptance is spelled out.

### Movement M1 — Worktrees: real isolation

> Replace branch-per-attempt with genuine `git worktree add` so N agents hold N
> physical checkouts. This is the single highest-leverage gap.

**Sprint 1.1 — `WorktreeManager` core**
- **Goal:** First-class git-worktree lifecycle in `lopi-git`.
- **Deliverables:** `git worktree add <path> -b <branch>` / `remove` / `prune`;
  worktrees rooted under `.lopi/worktrees/{task_id}-{attempt}`; auto-clean on
  drop (RAII guard) even on panic; reuse the existing scope/diff checker.
- **Key files:** `crates/lopi-git/src/worktree.rs` (new), `manager.rs` (delegate),
  `crates/lopi-core/src/config.rs` (`IsolationMode::{Branch,Worktree}`).
- **DoD:** property test — 8 concurrent worktree add/remove cycles leave zero
  orphan dirs and zero `git worktree list` leaks; `WORKTREE_LOCK` contention
  drops to *creation only*, not the whole run.

**Sprint 1.2 — Pool runs in worktrees**
- **Goal:** `AgentRunner` executes inside its worktree, not the shared root.
- **Deliverables:** thread the worktree path through `run_loop.rs`; per-worktree
  `CARGO_TARGET_DIR`; remove the global serialization now made unnecessary.
- **Key files:** `crates/lopi-agent/src/runner/run_loop.rs`,
  `crates/lopi-orchestrator/src/pool/run_loop.rs`.
- **DoD:** two tasks on the same repo build & test concurrently with no shared
  `target/` contention; wall-clock for 4 parallel tasks ≤ 1.6× a single task.

**Sprint 1.3 — Rebase-on-moved-main + branch GC**
- **Goal:** Loops survive a moving `main`; no branch litter.
- **Deliverables:** pre-PR `git rebase origin/main` with conflict → structured
  `TaskStatus::Conflict` (not silent fail); post-merge worktree+branch GC;
  `lopi worktree gc` CLI + dashboard button.
- **Key files:** `crates/lopi-git/src/worktree.rs`, `src/run_command.rs`.
- **DoD:** simulated mid-task upstream commit yields a clean rebase or an
  actionable `Conflict` with the conflicting paths; zero branches survive a
  merged PR.

### Movement M2 — Skills: a runtime engine

> Turn `.claude/skills/` from web-only metadata into a registry the lopi agent
> **loads, injects, audits, and grows**.

**Sprint 2.1 — `SkillRegistry` + loader**
- **Goal:** Parse `SKILL.md` (frontmatter: name, description, triggers, version)
  into a typed registry.
- **Deliverables:** `lopi-skill` crate; discovery from `.claude/skills/` and
  `.lopi/skills/`; semver per skill; validation (no dup names, schema-checked
  frontmatter).
- **Key files:** `crates/lopi-skill/src/{lib,parse,registry}.rs` (new).
- **DoD:** all 6 existing skills load; malformed frontmatter fails loudly with
  file+line, never silently.

**Sprint 2.2 — Relevance injection into the loop**
- **Goal:** The right skills enter the planning prompt automatically.
- **Deliverables:** trigger-match (keyword now, embedding-ready interface) →
  inject skill body into `AgentRunner` context; per-task **activation record**
  (which skill@version fed which task) in `lopi-memory`.
- **Key files:** `crates/lopi-agent/src/runner/mod.rs` (`with_skills`),
  `crates/lopi-memory/src/store/skills.rs` (new).
- **DoD:** a task whose goal matches a skill trigger shows that skill in its
  audit trail; no-match tasks inject nothing (no context bloat).

**Sprint 2.3 — Lesson → Skill promotion (self-evolving)**
- **Goal:** Close the Ratchet automatically — recurring lessons become named skills.
- **Deliverables:** detector (≥ N occurrences of a lesson cluster) → draft
  `SKILL.md` via a sub-agent → **human approval gate** → commit to `.lopi/skills/`.
- **Key files:** `crates/lopi-agent/src/skill_promotion.rs` (new),
  `crates/lopi-memory/src/store/lessons.rs`.
- **DoD:** a seeded triple-repeated lesson produces a draft skill PR; nothing
  auto-commits without approval; demotion path exists if the skill later
  correlates with regressions.

### Movement M3 — Plugins & connectors: MCP both ways

> The biggest categorical gap. lopi must **consume** external MCP servers (so
> the loop acts in your environment) and **expose** itself as one (so other
> agents drive lopi).

**Sprint 3.1 — MCP client**
- **Goal:** lopi agents can call tools from configured MCP servers.
- **Deliverables:** `lopi-mcp` crate (stdio + HTTP transports); server config in
  `.lopi/loop.toml` (`[[mcp.servers]]`); discovered tools merged into
  `lopi-tools::ToolRegistry`; per-server allowlist + timeout + circuit breaker
  (reuse `lopi-ratelimit`).
- **Key files:** `crates/lopi-mcp/src/{client,transport,registry}.rs` (new),
  `crates/lopi-tools/src/lib.rs` (wire `tool_use`).
- **DoD:** a reference MCP server (filesystem/github) is callable from a task;
  unreachable server degrades gracefully with `tracing::warn!`, never panics.

**Sprint 3.2 — MCP server (expose lopi)**
- **Goal:** External Claude Code / agents drive lopi over MCP.
- **Deliverables:** expose `submit_task`, `task_status`, `list_schedules`,
  `approve_plan`, `loop_health` as MCP tools; auth via existing allowlist model.
- **Key files:** `crates/lopi-mcp/src/server.rs`, `crates/lopi-ui/src/web/`.
- **DoD:** `claude mcp add lopi …` then a tool-call round-trips a real task; every
  tool has a JSON-Schema + doc string (zero undocumented public APIs).

**Sprint 3.3 — Connector plugin trait**
- **Goal:** New connectors without forking core.
- **Deliverables:** `Connector` trait (inbound events + outbound notify) with the
  existing Telegram/WhatsApp/GitHub re-expressed as implementations; durable
  outbound queue (replace fire-and-forget `tokio::broadcast` drops).
- **Key files:** `crates/lopi-remote/src/connector.rs` (new), refactor existing.
- **DoD:** Telegram + WhatsApp pass through the trait with byte-identical
  behavior; a dropped notification is retried, not lost.

### Movement M4 — Sub-agents: maker ≠ checker

> Depends on M1 (worktrees) + M3 (MCP). Make the verifier a **separate agent in a
> fresh session**, then enable shallow decomposition.

**Sprint 4.1 — True maker/checker split**
- **Goal:** The checker never sees the maker's chain-of-thought.
- **Deliverables:** verifier runs as a fresh sub-process/session against the
  maker's worktree diff only; structured verdict (pass/fail + reasons + score);
  feeds the existing L3/L4 autonomy gate.
- **Key files:** `crates/lopi-agent/src/runner/verifier_runner.rs`,
  `crates/lopi-agent/src/runner/run_loop.rs`.
- **DoD:** verifier context provably excludes maker transcript (test asserts
  isolation); measured score-vs-revert correlation improves over the shared-session baseline.

**Sprint 4.2 — Bounded task decomposition**
- **Goal:** One agent splits a large goal; children run in parallel worktrees.
- **Deliverables:** planner emits a small sub-task DAG (depth-capped); children
  dispatch through `AgentPool`; parent integrates; hard cap on fan-out + budget.
- **Key files:** `crates/lopi-orchestrator/src/pool/`, `crates/lopi-core/src/task.rs`.
- **DoD:** a 3-part goal completes as 3 parallel child runs + 1 integration; cap
  prevents runaway fan-out; partial-failure rolls up as a coherent parent status.

**Sprint 4.3 — Earned-trust auto-promotion**
- **Goal:** Schedules climb autonomy by demonstrated reliability (from `LOOP_ENGINEERING.md` §6 backlog).
- **Deliverables:** promote `AutonomyLevel` after N consecutive clean verified
  runs; **instant demote** on a post-merge revert; full audit.
- **Key files:** `crates/lopi-memory/src/store/schedules.rs`,
  `crates/lopi-orchestrator/src/schedule_manager.rs`.
- **DoD:** simulated clean streak promotes L2→L3; a seeded revert demotes within
  one tick; every transition is logged with cause.

### Movement M5 — Automations + Memory hardening

> Low-risk, runs in parallel with M1–M4.

**Sprint 5.1 — Webhook resilience**
- **Goal:** No dropped or duplicated triggers.
- **Deliverables:** event **dedup** (delivery-id idempotency), **dead-letter
  queue** for failed deliveries, async triage off the request path,
  schedule-change audit trail.
- **Key files:** `crates/lopi-webhook/src/github.rs`, `crates/lopi-memory/src/store/`.
- **DoD:** a doubly-delivered CI failure spawns exactly one task; a triage panic
  lands in the DLQ and is replayable.

**Sprint 5.2 — Event-payload templating**
- **Goal:** Schedules/webhooks parameterize goals from the event.
- **Deliverables:** safe template (`{{issue.title}}`, `{{ci.failed_job}}`) with
  injection-safe rendering.
- **Key files:** `crates/lopi-core/src/config.rs`, `crates/lopi-webhook/src/`.
- **DoD:** an issue-opened event yields a task goal carrying the issue title;
  template errors are validation-time, not run-time.

**Sprint 5.3 — External state (Ralph) + VISION anchor + stall detector**
- **Goal:** Continuity and a stop condition.
- **Deliverables:** per-loop markdown state file (`done` / `next`) the loop reads
  and updates each tick; `VISION.md` loaded as the intent anchor and propagated
  into every plan; `AgentEvent::ProgressStall` on semantic no-progress → halt.
- **Key files:** `crates/lopi-agent/src/runner/run_loop.rs`,
  `crates/lopi-core/src/config.rs`, `crates/lopi-memory/src/store/`.
- **DoD:** killing a loop mid-run and restarting resumes from the state file; a
  loop making no measurable progress for K ticks halts itself and reports.

### Movement M6 — The Loop Surface: unify + observe

> Reflect the finished primitives in one place (builds on `LOOP_ENGINEERING.md` §5 Option E).

**Sprint 6.1 — `GET /api/loop-engineering` aggregation + read-only Loop Lens**
- **Goal:** One screen: CLAUDE.md, skills (with versions), MCP servers, schedules,
  worktrees, autonomy levels, gates — read-only.
- **Key files:** `crates/lopi-ui/src/web/`, `web/src/routes/loop/`,
  macOS `LoopView`.
- **DoD:** every pillar's live state is visible without reading a TOML by hand.

**Sprint 6.2 — Loop-health dashboard**
- **Goal:** The visibility half of "production loop."
- **Deliverables:** 7-day sparklines (verifier pass-rate, score trend, lessons,
  skill activations, stalls), per-schedule budget estimate + cumulative spend,
  config-validity badge.
- **Key files:** `crates/lopi-memory/src/store/` (`LoopHealthStore`),
  `web/src/routes/loop/`.
- **DoD:** an operator can answer "is the loop healthy and what is it costing?"
  in one glance.

**Sprint 6.3 — Writable controls + per-loop token economics**
- **Goal:** Tune the loop from the surface, safely.
- **Deliverables:** per-schedule autonomy picker, skill enable/disable, MCP-server
  toggle, budget caps; wire `LoopConfig.budget_tokens` → Claude API `task_budget`
  so the model self-regulates instead of hard-cutting.
- **Key files:** `crates/lopi-ui/src/web/`, `crates/lopi-agent/src/runner/`.
- **DoD:** every writable control round-trips to `.lopi/loop.toml` (loop-as-code)
  and is reflected on next tick; budget changes take effect without restart.

---

## 5. Definition of Done — the whole Pentad

The initiative is complete when a single seeded scenario runs unattended:

> A cron **automation** fires at 09:00, calls a **triage skill** that reads the
> overnight CI failures and open issues and writes findings to the **external
> state file**. For each finding it opens an isolated **worktree**, dispatches a
> maker **sub-agent** to draft the fix and a separate checker **sub-agent** to
> grade it against the project **skills** and tests; on pass, a **connector**
> (MCP/GitHub) opens the PR and updates the ticket; the loop records what's done
> and what's next, and **halts** when the inbox is clear — escalating to a human
> only on a real question.

Acceptance: that scenario executes end-to-end on a fixture repo, every Konjo wall
green, with a loop-health screen showing the run and its cost.

---

## 6. Risks & mitigations

| Risk | Mitigation |
|------|-----------|
| Worktree disk/inode blowup under high fan-out | Hard cap on live worktrees + RAII GC + `lopi worktree gc`; budget halts before exhaustion |
| MCP server = new attack surface | Allowlist auth, per-tool scope, reuse constant-time verify; default-deny, opt-in per server |
| Decomposition runaway cost | Depth + fan-out caps, `task_budget`, no-progress stall detector — all *required* |
| Auto-promotion ships a bad change | Instant demote on revert, verifier isolation, L4 gated on score threshold |
| Self-evolving skills drift the harness | Promotion always behind a human-approval PR; demotion on regression correlation |
| Scope creep vs. shipped backbone | Each movement flag-gated and independently revertible; never breaks `cargo build` |

---

## 7. Sequencing summary

| Phase | Movements | Outcome |
|-------|-----------|---------|
| **17.1** | M1.1–1.3, M5.1–5.2 (parallel) | Real worktrees; resilient automations |
| **17.2** | M2.1–2.3, M3.1–3.3 (parallel) | Runtime skills; MCP both ways; connector trait |
| **17.3** | M4.1–4.3, M5.3 | Maker/checker; decomposition; earned trust; state + stall |
| **17.4** | M6.1–6.3 | Unified Loop surface + health + economics |

At the end of 17.4 every block is 🟢 and the §5 scenario passes.

---

## Sources

- Addy Osmani — *Loop Engineering* ([addyosmani.com/blog/loop-engineering](https://addyosmani.com/blog/loop-engineering/))
- Boris Cherny / Cat Wu — Claude Code creators on agent loops ([theneuron.ai](https://www.theneuron.ai/explainer-articles/claude-code-creators-boris-cherny-and-cat-wu-explain-how-to-use-agent-loops/))
- Times of India — Cherny: "days of AI prompts are over… time for loops"
- Cobus Greyling — *loop-engineering* patterns & CLI ([github.com/cobusgreyling/loop-engineering](https://github.com/cobusgreyling/loop-engineering))
- Companion: [`docs/LOOP_ENGINEERING.md`](./LOOP_ENGINEERING.md) (research synthesis + `LoopConfig` surface + shipped self-prompt engine)
