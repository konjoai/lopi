# Next — Report on Finish (Capability 3)

Sprint 2 (Skill Arguments) shipped: `Skill::render_body` +
`lopi_skill::parse_invocation`, wired at the CLI's `lopi run --goal`
boundary. See `LEDGER.md`'s Sprint 2 entry for the empty-arg and reuse
decisions, and `crates/lopi-skill/src/{lib.rs,invocation.rs}` for the code.

Per `PROMPTS_PLAN.md`'s sprint order, **Capability 3 (Report on Finish) is
next — isolate it as its own sprint.** Unlike Sprints 1 and 2 (additive,
single-crate), this one:

- Adds a real config-schema field: **`report: Option<String>` on
  `ScheduleEntry`** (`crates/lopi-core/src/config.rs:147`), accepting
  `"telegram"` (WhatsApp has no outbound-send path yet — see
  `PROMPTS_PLAN.md` capability 3 — so it isn't a valid value this sprint).
  Thread it onto **`Task`** the same way `autonomy_level` is already threaded
  from `ScheduleEntry` in `crates/lopi-orchestrator/src/scheduler.rs`
  (`task.autonomy_level = entry.autonomy_level;` — mirror that line for
  `report`).
- Crosses a crate + event-bus boundary that Sprints 1/2 never touched:
  `lopi-agent`'s `emit_report()` (`crates/lopi-agent/src/runner/finalize.rs`,
  L1 `ReportOnly` autonomy's report hook — currently only calls `self.log(...)`,
  a local tracing line) needs to reach `lopi-remote`'s already-built Telegram
  `bot.send_message` path (`crates/lopi-remote/src/telegram/notify.rs`),
  independent of the single global `chat_id` gate `notify_loop` currently
  hard-codes.

## Why this is riskier than Sprints 1/2

Sprints 1 and 2 never touched a serialized config schema (`template.rs` and
`Skill::render_body` are pure additions). This one does — `ScheduleEntry` is
TOML-serialized (`lopi.toml`) with existing round-trip expectations, so the
new field must be `#[serde(default)]` and validated the way
`LoopConfig::validate` already models other cross-field config invariants.

## Constraint carried forward

No new dependency should be needed — `lopi-agent` already depends on neither
`lopi-remote` nor vice versa today (check before assuming; if `lopi-agent`
would need to depend on `lopi-remote` to call `bot.send_message` directly,
that's the same kind of cross-crate-dependency decision Sprint 2 hit with
`lopi-skill` → `lopi-core`, and is worth flagging up front rather than
discovering mid-implementation — an event-bus-mediated design, where
`lopi-agent` only ever emits an `AgentEvent` and `lopi-remote`'s existing
subscriber loop reacts, likely avoids the new dependency entirely and fits
the existing `EventBus<AgentEvent>` architecture better than a direct call.
