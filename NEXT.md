# Next — Verifier as Explicit Gate (Capability 2)

Sprint 3 (Report on Finish) shipped: `ScheduleEntry::report` / `Task::report`,
`lopi_core::ReportChannel`, and `AgentEvent::ReportReady` wired from
`lopi-agent`'s `emit_report` to `lopi-remote`'s Telegram notifier over the
existing `EventBus<AgentEvent>` — zero new dependency edges. See `LEDGER.md`'s
Sprint 3 entry for the edge decision, the chat_id limitation, and why
`crates/lopi-core/src/event.rs` got split into `event.rs` /  `event_tests.rs`
/ `event_wire_format_tests.rs` along the way.

Per `PROMPTS_PLAN.md`'s sprint order, **Capability 2 (Verifier as Explicit
Gate) is next — and it is the LAST of the four capabilities in this recon.**
Land it only after this sprint's lessons are banked; it has the highest blast
radius of the four because it activates a wiring path that has never run in
production.

## What's already built (`lopi-agent/src/verifier.rs`, `lopi-memory/src/store/verifier.rs`)

`VerifierVerdict`, the `verifier_verdicts` table, and `VerifierAgent::verify`
(maker/checker isolation, rubric resolution, JSON verdict parsing) are solid
— reuse as-is. What's missing is everything that would make the gate
*explicit and configurable* instead of implicit and hardcoded:

- `VerifierAgent::verify` (`crates/lopi-agent/src/verifier.rs:108`) hardcodes
  the model: `self.client.complete(MODEL_OPUS, ...)` (line 119) — no per-call
  model or effort parameter.
- `AgentRunner::with_verifier()` (`crates/lopi-agent/src/runner/mod.rs:242`)
  sets a bool flag, but **has zero call sites anywhere in the workspace**
  (confirmed again this sprint: `grep -rn '\.with_verifier()' crates/` is
  empty). Today the *only* way to force the verifier is
  `autonomy_level >= VerifiedPr` (L3/L4, `requires_verifier` in
  `crates/lopi-agent/src/runner/finalize.rs:49`) — a repo-wide trust level,
  not a per-loop "require verifier" toggle independent of autonomy.

## What this sprint adds — name these fields so the work is unambiguous

- **`LoopConfig::verifier_model: Option<String>`** and
  **`LoopConfig::verifier_effort: Option<String>`** (`crates/lopi-core/src/loop_config.rs`,
  next to `autonomy_level` at line 208 — `#[serde(default)]`, following the
  same round-trip-safe pattern `report` used this sprint on `ScheduleEntry`).
  Mirror both onto **`Task`** the same way `autonomy_level` is mirrored
  (`crates/lopi-core/src/task.rs:196`) and `report` now is (`task.rs:205`).
- **`LoopConfig::verifier_required: bool`** (or equivalent — the PROMPTS_PLAN
  kill-test names it `verifier_required`) — a per-loop "require verifier pass"
  gate independent of `autonomy_level`, mirrored onto `Task` the same way.
- Parameterize `VerifierAgent::verify(..., model: &str, effort: Option<&str>)`
  instead of the hardcoded `MODEL_OPUS` constant.
- Wire pool construction (`crates/lopi-orchestrator/src/pool/`) to call
  `.with_verifier()` when `verifier_required` (or `verifier_model.is_some()`)
  is set — **the first real call site this path will ever have.**

## Why this is riskier than Sprints 1–3

Sprints 1–3 were additive (`template.rs`, `Skill::render_body`,
`AgentEvent::ReportReady`) or added one opt-in `#[serde(default)]` field.
This sprint activates a code path (`.with_verifier()` → the verifier
maker/checker flow) that has **never executed in production** — the first
real exercise of that flow happens the moment pool construction calls it, not
in a controlled test. Budget time for a careful kill-test proving
`verifier_enabled == true` end-to-end before wiring the pool, exactly as
PROMPTS_PLAN's capability-2 kill-test describes.

## Constraint carried forward

Both `LoopConfig` and `Task` have existing round-trip serde tests — new
fields must be `#[serde(default)]` and not break `loop_config_tests.rs` /
`lib.rs`'s `task_new_defaults`. No new dependency should be needed;
`VerifierAgent` already lives in `lopi-agent`, which already owns
`AgentRunner::with_verifier()`.
