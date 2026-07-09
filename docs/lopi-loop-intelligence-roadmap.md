# lopi loop-intelligence roadmap — from running loops to autonomous projects

**Owner:** Wes · **Repo:** github.com/konjoai/lopi
**Thesis:** lopi already has the *outer* loop system (scheduling, worktrees, skills, memory, guardrails, state, human checkpoints). What it's missing is the loop's *judgment core* — the ability to evaluate its own work, self-correct from that evaluation, and know when a goal is met. This roadmap builds that core, then goal-direction on top of it, then — as a governed stretch — autonomous decomposition and full project autonomy.

---

## The dependency spine (read this first)

Each layer needs the one below it. Skipping ahead is the classic failure: an autonomous planner sitting on a loop that can't reliably judge quality just decomposes faster into garbage, and compounds it every round.

```
C3  Project autonomy  (define a project → agents ship it)        ← stretch pinnacle
C2  Autonomous decomposition (plan → run → evaluate → re-plan)   ← stretch
C1  Assisted decomposition (propose a stack, human approves)
────────────────────────────────────────────────────────────
B1  Goal-directed stacks (run until the goal's evals pass)
────────────────────────────────────────────────────────────
A3  Progress-gating + termination (beats-best, no-progress stop, budget)
A2  Reflection / feedback (evaluator critique → next attempt; learnings)
A1  Eval execution + goal condition  ← THE KEYSTONE
────────────────────────────────────────────────────────────
NOW  outer loop system: schedule · worktrees · skills · memory · guardrails · verifier design
```

Everything above the line is blocked until **A1** ships. A1 is the item that's been sitting at the top of every NEXT_SESSION_PROMPT — the roadmap's job is to make clear it's not one feature among many, it's the floor the rest stands on.

---

## Track A — the judgment core (this is what makes lopi "self-directing")

### A1 · Eval execution + goal condition  — the keystone
**Goal:** evals stop being client-only intent and actually run, and a loop/stack can carry an explicit, machine-checkable success condition.
**Builds:** the 4-tier evaluator that executes — baseline (execution-ok) → test (shell exit) → judge (a **separate** model) → suite (KCQF). An explicit `goal`/acceptance object per loop and per stack that the evaluator checks against, so "done" is defined, not implied.
**Depends on:** nothing new — it's the floor.
**Kill-test / proof:** a loop with a failing test is scored FAIL and a passing one PASS, by an evaluator that is *not the author model*; the judge tier's verdicts are stable across reruns.
**Reuses:** the existing "never grade own homework" verifier design (separate model), the eval-ladder + KCQF definitions, worktrees for isolated check runs.
**Why it's first:** the research's single loudest warning is that evaluator-optimizer loops go circular when the evaluator can't tell good from bad — and lopi currently has *no evaluator running at all*. Independent verification is also the named fix for the structural "agents praise their own output" problem. This phase is both.

### A2 · Reflection / feedback routing
**Goal:** an until-loop becomes *reflect-and-retry*, not blind retry.
**Builds:** route the evaluator's critique + failure output into the next iteration's context (the Reflexion/self-refine pattern), and persist durable learnings across iterations and runs.
**Depends on:** A1 (you can't feed back an evaluation you don't have).
**Kill-test / proof:** on a task that fails first try, iteration 2's prompt visibly contains the evaluator's critique, and measured pass-rate with reflection beats blind-retry on a fixed task set.
**Reuses:** kohaku (episodic memory), pattern mining, SQLite — this is the "learnings/compounding loop" already flagged as kiban's biggest gap.

### A3 · Progress-gating + termination + budget enforcement
**Goal:** the loop moves *toward* a goal and stops cleanly instead of running out the clock or running away.
**Builds:** **beats-best / ratchet** (accept an iteration only if it improves against the eval), **no-progress detection** (stop or escalate after K non-improving rounds), and real **budget/cost enforcement** (un-hide budget once it actually limits).
**Depends on:** A1 (need a score to beat), ideally A2.
**Kill-test / proof:** a run that plateaus is halted by the no-progress detector rather than burning all iterations; a regression is rejected by the ratchet; a run that exceeds budget is stopped.
**Reuses:** the 30-run paired Wilcoxon + CoV rigor — that machinery *is* "did it actually beat best," now applied live instead of only in benchmarks.

**End of Track A, lopi can:** act → observe → evaluate (independently) → reflect → self-correct → iterate until a goal is met or progress stalls, unattended. That is the full definition of a self-directing loop.

---

## Track B — goal-directed stacks (uses the core)

### B1 · Goal-conditioned stacks
**Goal:** a stack runs *until its goal is satisfied*, not for a fixed count.
**Builds:** attach a stack-level goal/acceptance (the stack-control area we designed is the surface for it); the stack sequencer keeps looping/advancing the chain until the stack's evals pass or termination fires.
**Depends on:** A1–A3, and the stack-control work already in flight.
**Kill-test / proof:** a stack with a goal "benchmark ≥ X" loops until it hits X or the no-progress detector stops it — with the stop reason recorded.
**Reuses:** the stack model, the client sequencer, the purple stack-control area (goal lives there next to loop/schedule/limits).

**This is where the UI work and the intelligence work meet:** the stack-control area needs one more facet — the *goal* — and B1 is what makes "run stack" mean "pursue this outcome."

---

## Track C — autonomous decomposition & project autonomy (the stretch arc)

Marked stretch on purpose. Anthropic explicitly lists **over-engineered planning** as an antipattern, and a loop that plans and edits its own work crosses into self-improving territory that needs real governance. So this arc is deliberately gated behind Track A/B working, and each step keeps a human boundary until the layer below has earned trust.

### C1 · Assisted decomposition (human-in-the-loop)
**Goal:** given a goal + requirements, lopi *proposes* a stack of loops; the human approves/edits before it runs.
**Builds:** an orchestrator that reads a goal/requirements and emits a candidate stack (the orchestrator-workers pattern, but with a human gate). No autonomous execution yet.
**Depends on:** B1 (a proposed stack is only useful if stacks can pursue goals).
**Kill-test / proof:** for a real, scoped goal, the proposed decomposition is one a human would accept with minor edits — measured over several goals, not cherry-picked.
**Reuses:** the creation/multi-add flow (parked) becomes the *output surface* for proposals; the preset→eval sets seed each proposed loop.

### C2 · Autonomous decomposition (plan → run → evaluate → re-plan)
**Goal:** the orchestrator decomposes, runs, reads the evaluator's results, and adjusts the plan (adds/branches/reorders loops) — within guardrails, at high autonomy (L4 on the ladder).
**Builds:** the outer agentic loop closed at the *plan* level, not just the task level. Hard guardrails: step/cost ceilings, no-progress kill, and human checkpoints for irreversible actions.
**Depends on:** C1 + a *proven* A3 (autonomous re-planning without no-progress detection is a runaway-loop generator).
**Kill-test / proof:** on a task where the first decomposition is wrong, the system detects it from evals and re-plans to success — and on an unsolvable task, it *stops and escalates* rather than looping forever.
**Reuses:** the autonomy ladder (this is what L4 was always for), the verifier, worktrees for safe parallel exploration.
**Honest risk:** this is where the "loop that changes how work is done" boundary is; it needs the governance controls (audit, permissions, reversibility) the security guidance calls for before it touches anything real.

### C3 · Project autonomy — the pinnacle
**Goal:** define a project — requirements doc, goals, constraints, acceptance criteria — and the system decomposes it into stacks, executes, self-corrects, verifies against the requirements, and reports/ships.
**Builds:** a project layer above stacks (requirements → goals → stacks → loops), with the evaluator checking deliverables against the acceptance criteria, and heavy governance.
**Depends on:** all of the above, proven on progressively larger scopes.
**Kill-test / proof:** a small, real, self-contained project (with genuine acceptance tests) completed end-to-end with only checkpoint approvals — then honestly documented, including where it needed a human.
**Honest note:** this is the most speculative item on the plan and the one most likely to reveal that the *evaluator* is the ceiling on everything. If A1's evaluator can't reliably judge project-level acceptance, C3 can't work no matter how good the planner is. That's not a reason to skip it — it's the reason A1 is the keystone.

---

## Sequencing & how this meets the current work

- The **current sprints** (UI-2 → Backend-1 → Shell-1 → Stack-1) build the loop-*authoring and control* experience. This roadmap builds the loop-*intelligence*. They're complementary tracks.
- They **converge at B1**: goal-directed stacks need the stack-control area to express a goal, and need A1–A3 to pursue it.
- Recommended order once Stack-1 lands: **A1 → A2 → A3 → B1**, then re-evaluate whether the Track C stretch is worth starting or whether the goal-directed single-stack experience is already the product. Don't start C until A/B are proven on real work — the plan reserves the right to stop at B1 if that's where the value is.

## What every phase inherits from lopi today
Independent-verifier design, kohaku + pattern mining + SQLite memory, 30-run statistical rigor, worktrees, scheduling, the autonomy ladder, guardrails, and the kiban sprint discipline (which is itself a loop-engineering pattern — for you, not the agent).

## The honest through-line
lopi is already a real loop-engineering tool; the gap is judgment, not plumbing. Ship A1 and lopi becomes self-directing in the true sense. Everything in Track C is a bet on top of that — worth planning for, worth being genuinely excited about, and worth being disciplined enough to gate behind an evaluator you can actually trust.
