# Next — Loop-Stack UI (net-new frontend work)

Sprint 4 (Verifier as Explicit Gate) shipped: `LoopConfig`/`Task` gained
`verifier_required` / `verifier_model` / `verifier_effort`
(`#[serde(default)]`, round-trip-safe), `VerifierAgent::verify` is
parameterized (`model: &str, effort: Option<&str>`) instead of hardcoding
`MODEL_OPUS`, a pure `resolve_verifier` resolver enforces "never grade your
own homework" (defaults to a model that differs from the worker's), and pool
construction (`crates/lopi-orchestrator/src/pool/run_loop.rs`'s new
`build_runner`) now calls `.with_verifier()` — its first real call site ever
— when the gate is set. See `LEDGER.md`'s Sprint 4 entry for the
never-grade-your-own-homework default, why `verifier_effort` is a prompt hint
rather than a wire parameter, the pool-construction kill-test seam, and what
now exercises the previously-dead `.with_verifier()` path.

**All four recon capabilities from `PROMPTS_PLAN.md` are now landed:**
Prompt Templates (Sprint 1), Skill Arguments (Sprint 2), Report on Finish
(Sprint 3), and Verifier as Explicit Gate (Sprint 4). The recon punch list is
complete.

## What's next: the loop-stack UI, not more backend

The next body of work is **net-new frontend** — the Loop Engineering
cockpit surfaces (`web/src/lib/components/AgentPane.svelte` et al.) need to
expose the levers that now exist on the backend but have no UI:
`autonomy_level`, `verifier_required` / `verifier_model` / `verifier_effort`,
`report`, `promote_after` / `trust_ceiling`, `isolation`, and the skill/rule
enable lists on `LoopConfig`. `docs/LOOP_ENGINEERING.md`'s roadmap already
ranks "LoopConfig write path / editor" (`PATCH /api/loop-engineering` + a
config-editor UI) as the top impact-to-effort item — that's the natural
starting point.

This is explicitly **not** a backend sprint: no new `Task`/`LoopConfig`
fields, no new crate edges, no new gate-wiring. The schema surface this
sprint (and the three before it) built is the contract the UI now needs to
read and write against `web/src/lib/stores/`, `AgentPane.svelte`, and
whichever API handlers in `crates/lopi-ui/src/web/loop_handlers.rs` /
`schedule_handlers.rs` don't yet expose these fields. Audit those handlers
first — some of `LoopConfig`'s existing fields (e.g. `promote_after`,
`trust_ceiling`) may already lack API exposure, in which case the UI sprint
starts with closing that gap before it can build the editor.
