---
decays: state
verified-against: ca8e980
verified-date: 2026-07-28
---

# The Pentad — Loop Engineering Completion Roadmap

Verified against: ca8e980 · 2026-07-28 (re-verified; Sprint G's verification-gate work
(secrets-on-diff gate, dead-letter ledger, two-phase adversarial verifier — see
`LEDGER.md`'s Sprint G entry) landed on `main` since the prior 2026-07-27 check and
rewrote `crates/lopi-agent/src/verifier.rs`, adding a new `verifier_tests.rs`. Checked
every citation into files that sprint touched (`verifier.rs`, `verifier_cli.rs`,
`runner/finalize.rs`, `runner/mod.rs`, `pool/run_loop.rs`) — this round did not
re-check the full document, only citations into files changed since the prior
verification, same scope as that prior round's own check. Three line-number citations
had drifted (§1 "Sub-agents" row: `VerifierAgent::new`/`resolve_verifier` moved within
`verifier.rs`, and the `isolated_prompt_excludes_the_maker_plan` test moved to the new
`verifier_tests.rs`; §1 "Worktrees" row: `finalize.rs`'s conflict-mapping function
shifted ~15 lines) — fixed, no substance drift. `pool/run_loop.rs:380`'s
`setup_worktree` citation and `stop_reason.rs:27-28`'s `NoProgress` citation were
checked and are still exact. No DONE/PARTIAL/NOT-STARTED verdict changed anywhere in
§1 or §4. Prior round's own findings (nine citations, `with_skills`'s move to
`runner/builder.rs`, the `lopi-remote`/telegram description) still stand — see the
in-body history this line replaces in git blame.)

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

**This section was previously wrong.** A 2026-07-24 kill-test against `main` @
`63908a5` found four of the six blocks below misreported — each citing a
concrete capability (real `git worktree` isolation, an MCP client+server, a
runtime skill engine, maker/checker isolation) as missing when it was already
shipped. See `CHANGELOG.md` / `LEDGER.md` for the correction writeup. Every
cell below traces to a `file:line`, re-derived from the working tree, not
carried over from the prior version of this table.

**2026-07-25 re-verification:** every file:line citation in this section and §4
was re-checked against `main` @ `4d8418c` (30 commits past the prior
`63908a5` stamp). Only two files touched by that range intersected a cited
range — `crates/lopi-ui/src/web/mod.rs` (route registration moved from
`:307` to `:273` after an unrelated CORS-layer refactor) and `src/main.rs`
(`McpServe` moved from `:49,288` to `:50,299` after an `--insecure-no-auth`
flag was threaded through `main`) — fixed below. No status cell changed: no
commit in the range touched `crates/lopi-remote`'s connector surface,
`lopi-core::successor`, webhook dedup/DLQ, event-payload templating,
`earned_trust` wiring, or the loop-health/writable-controls gaps this
section calls out as still open.

**2026-07-26 re-verification:** re-checked against `71a470b` (Sprint F1's
verifier CLI backend + Sprint F3's log-persistence decoupling, merged
together). Unlike the 2026-07-25 pass, this range *did* touch two cited
files: F1 inserted a `Backend` enum ahead of `VerifierAgent` in
`verifier.rs`, shifting `VerifierAgent::new`'s `isolated: true` from
`:120-127` to **`:140-145`** and the isolation test from `:287` to
**`:331`** (§ below, and the Sub-agents row above); F3's persistence
decoupling shrank `run_loop.rs` by ~75 net lines, moving the
`setup_worktree` call site from `:414-444` to **`:338`** (Worktrees row,
above). Both fixed below/above. No status cell changed — the substance of
every claim (maker/checker isolation still defaults on, worktree isolation
still opt-in via `Branch`/`Worktree`) is unchanged, only citations moved.

**2026-07-27 re-verification:** re-checked against `fcc4988` — 22 commits since the
last stamp, none touching this doc's substance (a security/scope-lock sprint that
deleted the `lopi-app` crate entirely, and a web-composer sprint wiring
`autonomy_level`/`isolation`/`no_progress_limit` end to end). Nine citations drifted,
all pure movement, no status change: `loop_config.rs:29-35` → **`:38-44`**
(`IsolationMode`, TOCTOU-fix import shifted it) and `:160` → **`:141`**
(`skills_enabled`); `task.rs:442` → **`:467-472`** (`Task::from_template`, shifted by
new `no_progress_limit`/`isolation` fields); `src/main.rs:50,299` → **`:50,268`**
(`McpServe`, file shrank); `web/mod.rs:273` → **`:288`** (route registration);
`pool/run_loop.rs:338` → **`:380`** (`setup_worktree` call site, pushed down by the
new autonomy/isolation resolution block); `runner/mod.rs:329` (`with_skills`) moved
entirely to **`runner/builder.rs:92`** (file-size split); and
`crates/lopi-remote/src/lib.rs:1-10`'s description named `telegram`/`egress` modules
that have since been deleted (crate is now `whatsapp`-only at `:1-19`) — the
underlying verdict (no `Connector` trait, no durable outbound queue) is unchanged.
Every other citation checked (worktree.rs, rebase.rs, verifier.rs, successor.rs,
earned_trust.rs, the MCP client/server files, the loop-health/writable-controls
citations) matched exactly. No DONE/PARTIAL/NOT-STARTED verdict changed anywhere in
§1 or §4.

Legend: 🟢 solid · 🟡 partial · 🔴 missing.

| Block | Status | What exists | The true gap |
|-------|--------|-------------|--------------|
| **Automations** | 🟢 | `lopi-orchestrator` (`scheduler.rs`, `schedule_manager.rs`) cron; `lopi-webhook` CI-failure → task with HMAC verify; per-schedule autonomy L1–L4; run-history persistence | `crates/lopi-webhook/src/github.rs:36-60` — no delivery-id **dedup**, no **dead-letter queue**, triage is synchronous, no schedule-change audit trail. `crates/lopi-core/src/template.rs:44` has a generic `{name}`-hole templating primitive and `Task::from_template` (`crates/lopi-core/src/task.rs:467-472`) exists, but neither is called outside tests — event-payload templating is unwired scaffolding, not shipped |
| **Worktrees** | 🟢 | **Real `git worktree` isolation, shipped and wired.** `crates/lopi-git/src/worktree.rs:36-217` (`WorktreeManager` add/add_detached/prune/list/gc) with RAII `Drop` cleanup (`worktree.rs:295-330`); `crates/lopi-orchestrator/src/pool/worktree.rs:25-50` (`setup_worktree`) puts each task in its own detached worktree when `IsolationMode::Worktree` is set (`crates/lopi-core/src/loop_config.rs:38-44`), with per-worktree `CARGO_TARGET_DIR` (`worktree.rs:266-277`); `crates/lopi-git/src/rebase.rs:27-75` (`rebase_onto`/`rebase_onto_default`) rebases onto a moved default branch and maps conflicts to `TaskStatus::Conflict` (wired at `crates/lopi-agent/src/runner/finalize.rs:243-264`, `rebase_before_pr` — line drift from the Sprint G verification-gate work touching this file); GC exposed via `lopi worktree gc`/`list` (`src/worktree_commands.rs:18-51`) | Isolation mode defaults to `Branch`, not `Worktree` — a repo must opt in via `.lopi/loop.toml`. No mid-run snapshot |
| **Skills** | 🟢 | **Runtime skill engine, shipped and wired.** `crates/lopi-skill/src/registry.rs:17-93` (`SkillRegistry::load_from_dirs`, dup-name validation) parses `SKILL.md` frontmatter into a typed registry; `crates/lopi-agent/src/runner/builder.rs:92` (`with_skills` — moved out of `runner/mod.rs` since the last verification, file-size split) and `crates/lopi-agent/src/runner/seed.rs:210-241` (`seed_skills`/`record_skill_activation`) inject matching skills into the planning prompt and record activation | Lesson→skill promotion is **partial**: `crates/lopi-skill/src/promote.rs:37-60` (clustering) and `promoter.rs:40-60` (drafts to `.lopi/skills-pending/`, human-approval gate) exist and are reachable via `src/skill_commands.rs:64`, but drafting is a fixed string template, not "via a sub-agent" as originally scoped, and nothing triggers it automatically — it's a manual CLI-only path today |
| **Plugins & connectors** | 🟢 | **MCP client + server, shipped and wired — both directions.** `crates/lopi-mcp/src/client.rs:36-65` + `config.rs:19-37` (`[[mcp.servers]]` in `.lopi/loop.toml`) + `bridge.rs:21-49` (merges discovered tools into `lopi-tools::ToolRegistry`) is the consuming side; `crates/lopi-mcp/src/server.rs:18-80` wired at `src/mcp_commands/mod.rs:117-243` exposes `lopi_submit_task`/`lopi_get_task`/`lopi_cancel_task`/`lopi_list_tasks`/`lopi_get_logs`/`lopi_get_agent_dag`/`lopi_get_stats` as MCP tools over stdio (`McpServe` registered at `src/main.rs:50,268`) — more surface than the original sprint scoped | `crates/lopi-remote/src/lib.rs:1-19` is now down to a single hardcoded `whatsapp` module — Sprint S10 Phase 4 removed the `telegram` transport entirely (the iOS/macOS app covers that use case now; the `TaskSource::Telegram` variant itself survives as a durable persisted enum, see `LEDGER.md`), and the `egress` allowlist module cited here previously has also since been deleted. Neither removal changes the verdict: **no `Connector` trait exists anywhere in the crate, no durable outbound queue.** The original claim ("connectors are hardcoded singletons") still holds, just with one fewer singleton than when this was last checked |
| **Sub-agents** | 🟢 | **Maker/checker split, shipped and wired.** `crates/lopi-agent/src/verifier.rs:196-199` — `VerifierAgent::new` defaults `isolated: true`; `resolve_verifier` (`verifier.rs:47`) forces a different model than the maker; test `isolated_prompt_excludes_the_maker_plan` moved to the new `crates/lopi-agent/src/verifier_tests.rs:54` when Sprint G split verifier's tests into their own file — still asserts a maker's plan text never reaches the verifier's prompt | No parallel task decomposition: `crates/lopi-core/src/successor.rs:1-27` is a depth-capped (3) **sequential** one-hop successor chain, not a sub-task DAG dispatched through `AgentPool`. Earned-trust auto-promotion exists as an isolated, tested state machine (`crates/lopi-core/src/earned_trust.rs:31-101`) but has zero callers outside its own module — not wired into `schedule_manager.rs`, not persisted |
| **Memory / state** | 🟡 | `lopi-memory` SQLite (patterns, lessons, audit, schedules); `CLAUDE.md` + rules; `LoopConfig` → `.lopi/loop.toml`. Stall detection exists in a narrower form than originally claimed missing: `StopReason::NoProgress` (`crates/lopi-core/src/stop_reason.rs:27-28`) + `ProgressGate` (`crates/lopi-agent/src/runner/progress.rs:20-55`) halts on score-delta stagnation | Still genuinely open: no `AgentEvent::ProgressStall` variant (only a string-convention reason), no per-loop external markdown state file (Ralph), no `VISION.md` intent anchor |

**Verdict (corrected 2026-07-24):** four of the five "loop-defining primitives"
this roadmap called missing — true worktrees, MCP (both directions), the
runtime skill engine, and maker/checker isolation — are shipped and wired on
`main`. What's actually left is narrower and different in kind from what §1
used to claim: a `Connector` trait + durable outbound queue, parallel task
decomposition, wiring the already-built earned-trust state machine into the
scheduler, webhook dedup/DLQ, and the Ralph state file + `VISION.md` anchor.
See §4 for per-sprint status.

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

**Estimated envelope:** 18 sprints originally scoped. As of 2026-07-24, 9 are
shipped (all of M1; 2.1–2.2; 3.1–3.2; 4.1; 6.1), 6 are partial (2.3, 4.3, 5.2,
5.3, 6.2, 6.3), and 3 are not started (3.3, 4.2, 5.1) — see §4 for the
per-sprint breakdown. A sprint = one PR-sized increment, every wall green.
Movements M1–M3 run partially in parallel across worktrees (dogfood).

---

## 4. Sprints

Each sprint lists **Goal · Deliverables · Key files · Konjo DoD**. DoD assumes
the standing Three-Wall gates; only sprint-specific acceptance is spelled out.

### Movement M1 — Worktrees: real isolation

> Replace branch-per-attempt with genuine `git worktree add` so N agents hold N
> physical checkouts. This is the single highest-leverage gap.

**Sprint 1.1 — `WorktreeManager` core**
- **Status: ✅ DONE.** `crates/lopi-git/src/worktree.rs:36-217` (add/add_detached/prune/list/gc), RAII `Drop` cleanup at `worktree.rs:295-330`, rooted under `.lopi/worktrees` (`worktree.rs:24`); `IsolationMode::{Branch,Worktree}` at `crates/lopi-core/src/loop_config.rs:38-44`.
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
- **Status: ✅ DONE.** `crates/lopi-orchestrator/src/pool/worktree.rs:25-50` (`setup_worktree`) wires per-task detached worktrees into the pool, called from `crates/lopi-orchestrator/src/pool/run_loop.rs:380`; per-worktree `CARGO_TARGET_DIR` at `crates/lopi-git/src/worktree.rs:266-277`.
- **Goal:** `AgentRunner` executes inside its worktree, not the shared root.
- **Deliverables:** thread the worktree path through `run_loop.rs`; per-worktree
  `CARGO_TARGET_DIR`; remove the global serialization now made unnecessary.
- **Key files:** `crates/lopi-agent/src/runner/run_loop.rs`,
  `crates/lopi-orchestrator/src/pool/run_loop.rs`.
- **DoD:** two tasks on the same repo build & test concurrently with no shared
  `target/` contention; wall-clock for 4 parallel tasks ≤ 1.6× a single task.

**Sprint 1.3 — Rebase-on-moved-main + branch GC**
- **Status: ✅ DONE.** `crates/lopi-git/src/rebase.rs:27-75` (`rebase_onto`/`rebase_onto_default`), conflicts mapped to `TaskStatus::Conflict` at `crates/lopi-agent/src/runner/finalize.rs:228-245`; GC at `crates/lopi-git/src/worktree.rs:157-216`; CLI at `src/worktree_commands.rs:18-51` (`lopi worktree gc`/`list`).
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
- **Status: ✅ DONE.** `crates/lopi-skill/src/lib.rs:40-57` (typed `Skill`), `crates/lopi-skill/src/registry.rs:17-93` (`SkillRegistry::load_from_dirs`, duplicate-name validation).
- **Goal:** Parse `SKILL.md` (frontmatter: name, description, triggers, version)
  into a typed registry.
- **Deliverables:** `lopi-skill` crate; discovery from `.claude/skills/` and
  `.lopi/skills/`; semver per skill; validation (no dup names, schema-checked
  frontmatter).
- **Key files:** `crates/lopi-skill/src/{lib,parse,registry}.rs` (new).
- **DoD:** all 6 existing skills load; malformed frontmatter fails loudly with
  file+line, never silently.

**Sprint 2.2 — Relevance injection into the loop**
- **Status: ✅ DONE.** `crates/lopi-agent/src/runner/builder.rs:92` (`with_skills` — moved out of `runner/mod.rs` since the last verification, file-size split); `crates/lopi-agent/src/runner/seed.rs:210-241` (`seed_skills`/`record_skill_activation`). Activation is recorded through the generic audit trail rather than a dedicated `lopi-memory/src/store/skills.rs` (that file doesn't exist) — functionally equivalent, different location than originally scoped.
- **Goal:** The right skills enter the planning prompt automatically.
- **Deliverables:** trigger-match (keyword now, embedding-ready interface) →
  inject skill body into `AgentRunner` context; per-task **activation record**
  (which skill@version fed which task) in `lopi-memory`.
- **Key files:** `crates/lopi-agent/src/runner/mod.rs` (`with_skills`),
  `crates/lopi-memory/src/store/skills.rs` (new).
- **DoD:** a task whose goal matches a skill trigger shows that skill in its
  audit trail; no-match tasks inject nothing (no context bloat).

**Sprint 2.3 — Lesson → Skill promotion (self-evolving)**
- **Status: 🟡 PARTIAL.** Detector + drafting exist: `crates/lopi-skill/src/promote.rs:37-60` (clustering), `promoter.rs:40-60` (writes to `.lopi/skills-pending/`, the approval gate), reachable via `src/skill_commands.rs:64`. Still missing: drafting is a fixed string template, not "via a sub-agent"; nothing triggers promotion automatically (CLI-only today).
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
- **Status: ✅ DONE.** `crates/lopi-mcp/src/client.rs:36-65` (`McpClient`/`StdioClient`), `crates/lopi-mcp/src/config.rs:19-37` (`[[mcp.servers]]` in `.lopi/loop.toml`), `crates/lopi-mcp/src/bridge.rs:21-49` (merges into `lopi_tools::ToolRegistry`).
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
- **Status: ✅ DONE — exceeds original scope.** `crates/lopi-mcp/src/server.rs:18-80` (`ToolHandler` trait + `serve`), wired at `src/mcp_commands/mod.rs:117-243` exposing `lopi_submit_task`, `lopi_get_task`, `lopi_cancel_task`, `lopi_list_tasks`, `lopi_get_logs`, `lopi_get_agent_dag`, `lopi_get_stats` — more tools than this sprint's minimal ask (`submit_task`/`task_status`/`list_schedules`/`approve_plan`/`loop_health`); `list_schedules`/`approve_plan` specifically are not present, everything else is covered or exceeded.
- **Goal:** External Claude Code / agents drive lopi over MCP.
- **Deliverables:** expose `submit_task`, `task_status`, `list_schedules`,
  `approve_plan`, `loop_health` as MCP tools; auth via existing allowlist model.
- **Key files:** `crates/lopi-mcp/src/server.rs`, `crates/lopi-ui/src/web/`.
- **DoD:** `claude mcp add lopi …` then a tool-call round-trips a real task; every
  tool has a JSON-Schema + doc string (zero undocumented public APIs).

**Sprint 3.3 — Connector plugin trait**
- **Status: ⬜ NOT STARTED.** `crates/lopi-remote/src/lib.rs:1-19` has only a single `whatsapp` module now (Sprint S10 Phase 4 removed `telegram`; the `egress` module cited at prior verification has also since been deleted) — no `Connector` trait anywhere in the crate, no durable outbound queue.
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
- **Status: ✅ DONE.** `crates/lopi-agent/src/verifier.rs:140-145` — `VerifierAgent::new` defaults `isolated: true`; test `verifier.rs:331` (`isolated_prompt_excludes_the_maker_plan`) proves a maker's plan text never reaches the verifier's prompt.
- **Goal:** The checker never sees the maker's chain-of-thought.
- **Deliverables:** verifier runs as a fresh sub-process/session against the
  maker's worktree diff only; structured verdict (pass/fail + reasons + score);
  feeds the existing L3/L4 autonomy gate.
- **Key files:** `crates/lopi-agent/src/runner/verifier_runner.rs`,
  `crates/lopi-agent/src/runner/run_loop.rs`.
- **DoD:** verifier context provably excludes maker transcript (test asserts
  isolation); measured score-vs-revert correlation improves over the shared-session baseline.

**Sprint 4.2 — Bounded task decomposition**
- **Status: ⬜ NOT STARTED.** `crates/lopi-core/src/task.rs` has no parent/child DAG fields. The only related mechanism is `crates/lopi-core/src/successor.rs:1-27` — a depth-capped (3) **sequential** one-hop successor chain, not a parallel decomposition dispatched through `AgentPool`.
- **Goal:** One agent splits a large goal; children run in parallel worktrees.
- **Deliverables:** planner emits a small sub-task DAG (depth-capped); children
  dispatch through `AgentPool`; parent integrates; hard cap on fan-out + budget.
- **Key files:** `crates/lopi-orchestrator/src/pool/`, `crates/lopi-core/src/task.rs`.
- **DoD:** a 3-part goal completes as 3 parallel child runs + 1 integration; cap
  prevents runaway fan-out; partial-failure rolls up as a coherent parent status.

**Sprint 4.3 — Earned-trust auto-promotion**
- **Status: 🟡 PARTIAL.** `crates/lopi-core/src/earned_trust.rs:31-101` — a complete, tested `EarnedTrust` state machine (`on_clean_run`/`on_failed_run`/`on_revert`) matching the spec. But it has zero callers outside its own module: not invoked from `schedule_manager.rs`, not persisted (no `trust_ledger` in `lopi-memory`). Built, not wired.
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
- **Status: ⬜ NOT STARTED.** `crates/lopi-webhook/src/github.rs:36-60` has no delivery-id dedup, no dead-letter queue, no async triage (triage is synchronous), no schedule-change audit trail.
- **Goal:** No dropped or duplicated triggers.
- **Deliverables:** event **dedup** (delivery-id idempotency), **dead-letter
  queue** for failed deliveries, async triage off the request path,
  schedule-change audit trail.
- **Key files:** `crates/lopi-webhook/src/github.rs`, `crates/lopi-memory/src/store/`.
- **DoD:** a doubly-delivered CI failure spawns exactly one task; a triage panic
  lands in the DLQ and is replayable.

**Sprint 5.2 — Event-payload templating**
- **Status: 🟡 PARTIAL.** `crates/lopi-core/src/template.rs:44` (`resolve`) is a generic `{name}`-hole templating primitive (single-brace, not `{{issue.title}}` syntax), and `Task::from_template` (`crates/lopi-core/src/task.rs:467-472`) exists — but neither has a caller outside `task_tests.rs`. Not wired into schedules or `lopi-webhook`.
- **Goal:** Schedules/webhooks parameterize goals from the event.
- **Deliverables:** safe template (`{{issue.title}}`, `{{ci.failed_job}}`) with
  injection-safe rendering.
- **Key files:** `crates/lopi-core/src/config.rs`, `crates/lopi-webhook/src/`.
- **DoD:** an issue-opened event yields a task goal carrying the issue title;
  template errors are validation-time, not run-time.

**Sprint 5.3 — External state (Ralph) + VISION anchor + stall detector**
- **Status: 🟡 PARTIAL.** Stall detection exists in narrower form: `StopReason::NoProgress` (`crates/lopi-core/src/stop_reason.rs:27-28`) + `ProgressGate` (`crates/lopi-agent/src/runner/progress.rs:20-55`) halts on score-delta stagnation, but there's no `AgentEvent::ProgressStall` variant (only a string-convention reason in `guardrails.rs:17`). Still missing entirely: no `VISION.md` intent anchor, no per-loop external markdown state file.
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
- **Status: ✅ DONE.** `crates/lopi-ui/src/web/loop_handlers.rs:1-60` aggregates config/skills/rules/schedules; route registered at `crates/lopi-ui/src/web/mod.rs:288`; consumed by `web/src/routes/loop/+page.svelte` (685 lines).
- **Goal:** One screen: CLAUDE.md, skills (with versions), MCP servers, schedules,
  worktrees, autonomy levels, gates — read-only.
- **Key files:** `crates/lopi-ui/src/web/`, `web/src/routes/loop/`,
  macOS `LoopView`.
- **DoD:** every pillar's live state is visible without reading a TOML by hand.

**Sprint 6.2 — Loop-health dashboard**
- **Status: 🟡 PARTIAL.** `crates/lopi-ui/src/web/loop_health_handlers.rs:26-50` (`GET /api/loop-engineering/health` — stats/attempts/outcomes/burn series) consumed at `web/src/routes/loop/+page.svelte:50,102-106,234-341`; config-validity badge confirmed (`+page.svelte:450-451,488`). Missing: series are windowed by attempt count (60), not calendar 7-day; no per-schedule budget estimate anywhere in the handler or frontend.
- **Goal:** The visibility half of "production loop."
- **Deliverables:** 7-day sparklines (verifier pass-rate, score trend, lessons,
  skill activations, stalls), per-schedule budget estimate + cumulative spend,
  config-validity badge.
- **Key files:** `crates/lopi-memory/src/store/` (`LoopHealthStore`),
  `web/src/routes/loop/`.
- **DoD:** an operator can answer "is the loop healthy and what is it costing?"
  in one glance.

**Sprint 6.3 — Writable controls + per-loop token economics**
- **Status: 🟡 PARTIAL.** Real writable controls exist: per-schedule autonomy picker (`crates/lopi-ui/src/web/schedule_handlers.rs:202`, `POST /api/schedules/:id/autonomy`), self-prompt strategy (`loop_handlers.rs:233-254`), escalation toggle (`loop_handlers.rs:258-269`). Missing: no skill enable/disable toggle (despite `LoopConfig.skills_enabled` existing at `crates/lopi-core/src/loop_config.rs:141` — the web panel is read-only), no MCP-server toggle in the web UI, no write path for `task_budget` caps (backend plumbing exists at `crates/lopi-agent/src/api_budget.rs`, but the loop page only displays it read-only).
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
