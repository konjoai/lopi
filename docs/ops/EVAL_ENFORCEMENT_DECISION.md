# EVAL_ENFORCEMENT_DECISION — what "the evaluator lands server-side" actually means today

**Baseline:** `origin/main` @ `43f7cd5` (Loop Stack connect & test, v0.11.0) · **Date:** 2026-07-15
**This is a decision aid, not a decision.** No code in this repo changed as a result of writing it — `CreateTaskBody`/`launchStackTask` are untouched. Its job is to correct the record on what's actually built (several standing docs turn out to be wrong about this, not just imprecise) and lay out what's genuinely still open.

## The headline correction

`NEXT_SESSION_PROMPT.md` and the `Loop Stack connect & test` / `macOS-Loop-Stacks-1` ledger entries all state that `acceptance`/`budget_tokens` are "carried in the pure payload, unit-tested, never wired to the request body" and that this is "A1–B1's evaluator track (no backend changes)." **That is only true for macOS, and even there it's a bug, not a scope decision.** Re-reading the ledger (as this phase's brief asked) surfaced the claim; re-verifying it against the current code (not just re-reading the claim) shows it doesn't hold:

- **Server**: `CreateTaskRequest.acceptance` and `.budget_tokens` exist and are applied. Landed in A1 (`58a8ece`, commit message: *"CreateTaskRequest gains acceptance + verifier_fail_open"*) and A3 (`13404b0`, commit message: *"budget_tokens overrides the repo default, wired through CreateTaskRequest"*). The handler copies both straight into the constructed `Task`:
  `crates/lopi-ui/src/web/handlers.rs:290-291` (`if let Some(a) = &req.acceptance { task.acceptance = Some(a.clone()); }`) and `:296-297` (`if let Some(b) = req.budget_tokens { task.budget_tokens = b; }`). Real, load-bearing, since A1/A3 — not "no backend changes."
- **Web client**: also wired. `stores/stack.ts::cardToTaskPayload` (the function the run-stack sequencer calls for every card launch) computes both (`web/src/lib/stores/stack.ts:910-915`) and sets them on `CreateTaskOptions`. `api.ts::createTask` spreads the whole options object into the actual POST body (`json('POST', { goal, priority, ...(repo ? {repo} : {}), ...opts })`, `web/src/lib/api.ts:165`) — so anything present on `options` reaches the wire, no second allowlist to fall out of sync with. **Web has sent `acceptance`/`budget_tokens` on real run-stack launches since A1/A3.**
- **macOS client**: genuinely not wired — but not for the reason every doc gives. `StackPayload.swift::cardToTaskPayload` (the 1:1 Swift port, `packages/LopiStacksKit/Sources/LopiStacksKit/StackPayload.swift:130-148`) computes `options.acceptance`/`options.budgetTokens` identically to web. But `launchStackTask` (`macos/Lopi/Store/AppModel+Stacks.swift:53-68`), which builds the *actual wire struct* from that pure payload, does a field-by-field manual mapping that simply omits both — and `CreateTaskBody` (`macos/Lopi/Networking/Models.swift:137-180`) doesn't even declare the properties. The struct's own comment (`Models.swift:157-159`) says *"deliberately NOT mapped here... same honesty gap as web"* — that comment is what every later doc trusted instead of the code. It was wrong when written (web didn't have this gap even then) and has been repeated at least three times since (`macOS-Loop-Stacks-1`, `NEXT_SESSION_PROMPT.md`, `Loop Stack connect & test`'s own Phase 1 skip) without anyone re-checking it against `stack.ts`.

**Why this matters beyond pedantry:** a caller reading only the docs would conclude "the evaluator needs a server-side landing decision" and could spend a sprint re-deciding something that already shipped. The actual remaining gap is narrow, macOS-only, and mechanical — not an open design question.

## What the macOS fix actually needs (not done here — Phase 3 is prep, not wiring)

Two layered gaps, not one:

1. **`budgetTokens: Int?`** — trivial. Add the property to `CreateTaskBody`, a `budget_tokens` `CodingKey`, and pass `o.budgetTokens` through in `launchStackTask`. No type-system work; it's already a plain integer.
2. **`acceptance`** — not trivial in the same way. `StackAcceptance`/`AcceptanceCheck`/`AcceptanceSpec` (`StackPayload.swift:18-51`) are only `Equatable` today, not `Codable`. The Rust wire shape they'd need to match is a serde-tagged union (`crates/lopi-core/src/acceptance.rs:125`, `#[serde(tag = "kind", rename_all = "snake_case")]` on `CheckSpec`) — `AcceptanceSpec.executionOk`/`.judge(rubricName:criteria:)`/`.suite(name:)` would need hand-written `Codable` conformance (a plain `Codable` synthesis won't produce the tagged-union shape `execution_ok`/`judge`/`suite` the server's `serde` deserializer expects), not just a property add. Real, scoped, boundable work — smaller than "build an evaluator," bigger than "forgot a field."

## A separate mechanism this does NOT affect: chain (stack-level) acceptance

Don't conflate the above with `StackConfig.evals`/chain-level goal pursuit — that's real, working, and was never meant to be a `CreateTaskRequest` field. Per-card acceptance (above) scores one loop's own pass/fail via the `TieredEvaluator` during that task's own finalize. Chain acceptance is client-side orchestration: `runStack`'s goal pursuit spawns a *separate* verify task (`client_ref="s1::stack-eval::0"` per Verify-4's live capture) once the chain completes, and reads that task's own outcome. `docs/ops/LIVE_UI_STATUS_FINAL.md`'s Verify-4 confirmed this live: "chain acceptance is evaluated by spawning a real verify task... never by wiring an acceptance field" — that statement is correct and unrelated to the per-card gap above. No decision needed here; it's already the deliberate, working design.

## What's actually still open (the real decision, reframed)

Since "does the evaluator run server-side" is already answered (yes, since A1), the genuinely open question is narrower: **today, `acceptance` is purely opt-in per task** — a task built with no evals selected carries `acceptance: None`, and `TieredEvaluator` never runs at all; finalize falls back to the pre-A1 `score.passed()` heuristic (`stores/stack.ts:855-856`'s own comment: *"falls back to the legacy `score.passed()` gate... unchanged for a card that somehow carries no evals"*). Three framings of what "enforcement" could mean from here, each with a real cost:

1. **Stay opt-in (today's behavior, no change).** A caller who wants real evaluation selects evals; one who doesn't gets the legacy heuristic. Requires nothing. Cost: nothing stops a caller (human or automated — MAXX, a schedule, a webhook-triggered task) from shipping unevaluated work silently; there's no floor.
2. **A repo-level or global default acceptance.** `.lopi/loop.toml` (or a new config surface) declares a baseline check (e.g. `execution_ok`) applied to every task unless explicitly overridden. Requires: a config schema addition, a precedence rule (task-level `acceptance` vs. repo default — presumably task wins, mirroring the existing `loop.field ?? stack.default.field ?? DEF.field` pattern A1's own ledger entry already established for other fields), and deciding whether existing repos silently gain enforcement on upgrade (a real behavior change for anyone not expecting it) or must opt in once.
3. **Mandatory acceptance for specific dispatch paths only.** E.g., require `acceptance` be present for any task MAXX or a cron schedule creates (unattended paths), leave interactively-created tasks opt-in. Requires: gating logic at each dispatch site (`maxx_loop.rs`, `scheduler.rs`) rather than one central rule — more surface area, but doesn't change the default for a human at the keyboard who may legitimately want a quick unscored run.

No recommendation is made here between these three — that's the actual decision this doc defers, not whether the evaluator exists server-side.

## For whoever picks this up next

- The macOS gap (mechanical, ~budget_tokens trivial / acceptance real-but-scoped) is worth its own small follow-up regardless of which enforcement framing is chosen — it's a bug today, not blocked on any decision.
- Framings 2 and 3 above are the actual open design question; framing 1 is "no code needed," included for completeness, not as a straw man.
- The three ledger entries that repeat the incorrect "no backend changes" claim are not corrected in place here (this repo's ledger is append-only, newest-first, not amended) — flagged so a future docs pass can fix it, not fixed by this one.
