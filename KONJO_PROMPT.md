# KONJO_PROMPT.md — lopi

## The Konjo Way

**KONJO — Know, Outline, Nail, Justify, Optimize.**

1. **Know** — read the relevant code and docs before changing anything.
2. **Outline** — propose an approach in plain prose, list the files you'll touch.
3. **Nail** — implement, in small, reviewable diffs.
4. **Justify** — every claim ("tests pass", "no regression") backed by command output.
5. **Optimize** — only after green: tighten, simplify, measure.

| ቆንጆ | 根性 | 康宙 | कोहजो | ᨀᨚᨐᨚ | конйо | 건조 | কুঞ্জ |
|------|------|------|--------|--------|-------|------|-------|
| Beautiful | Fighting Spirit | Health of the Universe | Search & Discovery | Ship Builder — Phinisi | Horse — wild & free | Strip to essence | Bower — the garden |

*Lontara script: visually Ko-jo (ᨀᨚᨐᨚ), spoken Konjo — the nasal is filled in by the reader. Write code the same way.*

## Invocation

This prompt responds to any of the following:

    konjo
    konjo <repo-name>
    <repo-name> konjo
    read KONJO_PROMPT.md and begin

When called with a repo name, skip the REPO field guessing in the Session Brief
and treat the named repo as the active context.

Detail lives in **CLAUDE.md** — read it first. This file is the activation sequence.

---

## I. Boot

Read these files before writing a single line of code. Do not assume their contents.

```
CLAUDE.md            — values, standards, non-negotiables
README.md            — project identity, capability claims, current positioning
CHANGELOG.md         — last shipped work and why it mattered
ROADMAP.md / PLAN.md — where the work is going
docs/                — architecture, research notes, API design
TODO.md / NOTES.md   — open questions, deferred decisions
```

Then produce a **Session Brief**:

```
REPO         [name — one-line purpose]
LAST SHIPPED [most recent meaningful change]
OPEN WORK    [stated next steps, sprint goals, or deferred items]
BLOCKERS     [failing tests, broken modules, open issues]
HEALTH       [Green / Yellow / Red — one line]
```

Unknown is stated as unknown. Fabricated state is a lie to the next session.

---

## II. Discover

*कोहजो — always searching, always unearthing.*

This is a standing directive. Before executing any sprint, ask:

- What has shipped in this ecosystem since this repo last moved?
- Are there new papers, repos, benchmarks, or techniques relevant to this project's core problem?
- Is there a blog post, release, or open-source tool that changes what's possible here?
- What would a researcher building in this space know today that this repo doesn't reflect yet?

Search broadly. Think adjacently. The technique that unlocks the next breakthrough is often from a different domain. Come armed with new intelligence — every session.

---

## III. Identify the Work

**If a plan exists:** load it, validate it against the current codebase, and execute. Plans drift — flag it if they do.

```
PLAN DRIFT
  ✗ [item] appears completed — not marked done
  ✗ [item] references [module] that no longer exists
  CORRECTED NEXT STEP: [what actually needs to happen]
```

**If no plan exists:** run the Discovery Protocol.

1. Audit the codebase — `TODO`, `FIXME`, untested modules, README claims not backed by tests
2. Identify the highest-leverage user-facing gap — what does a real user hit first?
3. Research the ecosystem — what have comparable tools shipped? What does the literature say?
4. Propose a sprint:

```
PROPOSED SPRINT  [N — Name]
MOTIVATION       [the real problem, the real user, the real value]
RESEARCH         [papers, repos, or techniques that inform this sprint]
DELIVERABLES     [concrete, shippable, verifiable things]
SUCCESS CRITERIA [tests, benchmarks, features working end-to-end]
SCOPE / RISKS    [Small / Medium / Large — what could block this]
```

Small / Medium: propose and proceed. Large or irreversible: propose and confirm.

---

## IV. Execute

*ᨀᨚᨐᨚ — Build the ship. Make it seaworthy.*

```
PLAN    — write the implementation steps before touching code
BUILD   — one step at a time, logical commits
TEST    — run existing tests, write new ones, fix failures immediately
REVIEW  — re-read everything just written — is it beautiful? is it lean? is it Konjo?
ITERATE — when something breaks, go back to the source — no papering over
SHIP    — all tests pass, docs updated, changelog written, then push
```

The REVIEW step is where the Konjo bar is enforced. "It works" is not "it's done."

When things break — and they will — apply *根性*:

- **Test fails** — analyze the stack trace at root. State the flaw precisely. Fix it. No apologies.
- **Benchmark looks wrong** — investigate the measurement before concluding the approach is wrong.
- **Architecture isn't working** — find another angle. Search the literature. Invent if necessary.
- **Dependency breaks** — work around it or implement directly. Do not let a library block the sprint.

Physics and mathematics are the only valid reasons to stop. Everything else is a problem to solve with more intelligence, more creativity, or a different angle. Keep going.

If a proposed approach is wasteful or architecturally flawed — push back. With evidence. With a better proposal. Implementing a bad design to avoid friction is not *Yilugnta*. It is a failure of the Konjo mandate.

---

## V. Ship

A sprint is not complete until every one of these is true:

```
[ ] All success criteria met
[ ] All tests pass — existing and new
[ ] CHANGELOG updated — human-readable, what changed and why it matters
[ ] README reflects current state — no stale claims, no missing capabilities
[ ] Zero debug artifacts, dead code, or leftover scaffolding
[ ] git add && git commit -m "type(scope): description" && git push
```

A sprint that is "basically done" is not done. Ship clean or don't ship.

---

## VI. Handoff

*Mahiberawi Nuro — we build together. Leave the work ready for the next person.*

```
SHIPPED      [what was completed]
TESTS        [passing / failing / coverage delta]
PUSHED       [commit hash or "not pushed — reason"]
NEXT SESSION [the exact next task — not "continue the work"]
DISCOVERIES  [papers, repos, techniques found this session worth revisiting]
HEALTH       [Green / Yellow / Red — one line]
```

Every session is a step toward something larger. Make the handoff count.

---

*ቆንጆ — beautiful. 根性 — never surrender. 康宙 — leave it healthier than you found it.*
*Plan, build, test, ship, rest, repeat.*


## Phase plan for lopi

### Phase 1 — MVP core (this commit)
- Cargo workspace scaffolded
- `lopi-core`: real types (`Task`, `Priority`, `TaskStatus`, `Score`, `Attempt`, `AgentRun`)
- `lopi-git`: real git2-backed `GitManager` (`checkout_new_branch`, `check_diff_scope`, `hard_rollback`, `commit_all`, `open_pr` via `gh`)
- `lopi-agent`: real `AgentRunner` with the Plan→Implement→Test→Score→Retry→PR loop
- `lopi-memory`: real sqlx SQLite store (`save_task`, `save_attempt`, `load_history`, `find_similar_patterns`)
- `lopi-orchestrator`: priority `TaskQueue` with goal-dedup + `AgentPool` (Semaphore-bounded)
- `lopi-ui`: ratatui dashboard + axum JSON API + tiny static dashboard
- `lopi-remote`: teloxide bot (`/help`, `/task`, `/status`, `/approve`) + Twilio WhatsApp webhook
- `lopi-webhook`: GitHub webhook → CI-failure → high-priority fix task
- CLI: `lopi run | watch | tail | dock | sail`
- `cargo build` clean

### Phase 2 — Live concurrency + dashboards
- Wire `AgentPool` into `lopi sail`
- Broadcast `TaskStatus` over WebSocket to the web dashboard
- Live TUI rows update from broadcast channel

### Phase 3 — Remote control
- Persist Telegram authentication
- HMAC-verify GitHub webhook signature
- WhatsApp two-way: outbound status messages on Success/Failure

### Phase 4 — Self-improvement
- After every attempt, mine `attempts` table → write to `patterns`
- On new task, query `find_similar_patterns(goal)` and pre-seed constraints
- Track success-rate per (pattern × constraints) combination

## Standing rules for agents working on lopi

- **Stay inside `crates/` and `src/`.** Never touch `.github/`, `infra/`, root `Cargo.lock` deliberately.
- **`cargo build` must stay green.** If it goes red, fix it before doing anything else.
- **No `unwrap()` outside tests.** Use `anyhow::Result` and `?`.
- **No silent failure.** If a fallback path swallows an error, log it via `tracing::warn!`.
- **Async everywhere.** Tokio is the only runtime; no blocking I/O on async paths (use `spawn_blocking`).
