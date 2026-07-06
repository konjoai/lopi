# PROMPTS_PLAN.md — Recon: Templates, Verifier Gate, Report-on-Finish, Skill Arguments

**Scope note:** This branch (`claude/chat-render-living-orb-713jf5`) and `origin/main` have
diverged (19 commits ahead here, 17 ahead on main). Checked both. The files that matter for
these four capabilities — `crates/lopi-core/src/{task.rs,config.rs,loop_config.rs}`,
`crates/lopi-memory/src/store/verifier.rs`, `crates/lopi-agent/src/verifier.rs`, and all of
`crates/lopi-remote/`  — are **byte-identical** between the two branches, so every finding below
holds on both. The only relevant divergence: `origin/main` has `crates/lopi-skill/src/promote.rs`
+ `promoter.rs` (Pentad M2.3, "lesson → skill promotion") which **does not exist on this branch**
and is a different feature from what's being asked for below (see Capability 4). Also confirmed
on `origin/main`: zero call sites of `.with_verifier()` anywhere in `crates/` — same as this
branch (see Capability 2).

`crates/lopi-core/src/loop_config_tests.rs` and `crates/lopi-skill/src/registry.rs` /
`crates/lopi-skill/src/lib.rs` were also read; `crates/lopi-skill/src/promote.rs` does not exist
on this branch (confirmed via `Read` error, then via `git ls-tree` against this branch).

---

## Summary Table

| Capability | Exists today (file:lines) | Missing | Smallest change | Est. new modules/files | Risk to existing schema |
|---|---|---|---|---|---|
| **1. Prompt Templates** | Nothing. `Task::new(goal: impl Into<String>)` at [task.rs:229](crates/lopi-core/src/task.rs:229) takes a literal string. Every producer builds `goal` via `.clone()`/`format!` before calling it: [telegram/handlers.rs:171,267,387](crates/lopi-remote/src/telegram/handlers.rs), [whatsapp.rs:93](crates/lopi-remote/src/whatsapp.rs:93), [webhook/github.rs:140,175](crates/lopi-webhook/src/github.rs), [webhook/issue.rs:169](crates/lopi-webhook/src/issue.rs:169), [orchestrator/queue.rs:190](crates/lopi-orchestrator/src/queue.rs:190), [orchestrator/schedule_manager.rs:69](crates/lopi-orchestrator/src/schedule_manager.rs:69), [orchestrator/scheduler.rs:30](crates/lopi-orchestrator/src/scheduler.rs:30), [lopi-ui/web/handlers.rs:233](crates/lopi-ui/src/web/handlers.rs:233). No `{var}` scanner anywhere (`grep -rli template\|substitut` across `crates/` and `web/src` hits only unrelated UI files — HTML placeholder, layout tiling, onboarding copy). | A hole-filling function + a call site that runs it *before* `Task::new`. | Add `crates/lopi-core/src/template.rs`: pure `fn resolve(template: &str, vars: &HashMap<String, String>) -> anyhow::Result<String>` — literal `{name}` substitution, `Err` on unresolved hole (loud failure per CLAUDE.md, no silent passthrough). Callers that want template semantics call `resolve()` then `Task::new(resolved)` — no `Task`/queue schema touched at all. | 1 new file (+ its test module) | **Low** — purely additive function; `goal: String` field unchanged. |
| **2. Verifier as Explicit Gate** | `VerifierVerdict` + `verifier_verdicts` table: [store/verifier.rs](crates/lopi-memory/src/store/verifier.rs) (persistence only, already solid). `VerifierAgent::verify` hardcodes the model: [agent/verifier.rs:119](crates/lopi-agent/src/verifier.rs:119) `self.client.complete(MODEL_OPUS, ...)` — no per-call model/effort param. `AgentRunner::with_verifier()` sets a bool flag: [runner/mod.rs:240-243](crates/lopi-agent/src/runner/mod.rs:240). Gate logic: [runner/finalize.rs:49-51](crates/lopi-agent/src/runner/finalize.rs:49) `requires_verifier(verifier_enabled, level) = verifier_enabled \|\| level.requires_verifier()`, where `level` is `Task::autonomy_level` (L3/L4 force it: [loop_config.rs:91-95](crates/lopi-core/src/loop_config.rs:91)). | `.with_verifier()` has **zero call sites** anywhere in the workspace outside its own definition (`grep -rn "\.with_verifier()" crates/` → empty, confirmed on both branches). So today the *only* way to force a gate is `autonomy_level >= VerifiedPr`, which always uses the hardcoded Opus model with no effort control, and is a repo-wide/task-wide trust level, not a per-loop-step "require verifier" toggle independent of autonomy. | Add `verifier_model: Option<String>` + `verifier_effort: Option<String>` to `LoopConfig` ([loop_config.rs:204](crates/lopi-core/src/loop_config.rs:204), `#[serde(default)]`) and mirror onto `Task` (same pattern as `autonomy_level` at [task.rs:194-195](crates/lopi-core/src/task.rs:194)). Parameterize `VerifierAgent::verify(..., model: &str)` instead of the hardcoded constant. Wire pool construction to call `.with_verifier()` when the field is set (first real call site ever). | 0 new files; edits to `loop_config.rs`, `task.rs`, `agent/verifier.rs`, `runner/mod.rs`, pool construction site (`crates/lopi-orchestrator/src/pool/`). | **Medium** — touches `Task` and `LoopConfig`, both serialized with existing TOML/JSON round-trip tests. Additive `#[serde(default)]` fields keep old configs loading, but it's real schema surface. |
| **3. Report on Finish** | Telegram send path exists and works: [telegram/notify.rs:159-163](crates/lopi-remote/src/telegram/notify.rs:159) `bot.send_message(chat_id, text)`, subscribed to the `EventBus<AgentEvent>` in [notify.rs:15-29](crates/lopi-remote/src/telegram/notify.rs:15). L1 `ReportOnly` autonomy already has a distinct "report" hook: `emit_report()` at [runner/finalize.rs:163-170](crates/lopi-agent/src/runner/finalize.rs:163). | `emit_report()` only calls `self.log(...)` (local tracing/dashboard line) — it never reaches Telegram/WhatsApp. `notify_loop` sends to exactly one hardcoded `chat_id: Option<i64>` from `RemoteConfig::telegram.chat_id` ([config.rs:91](crates/lopi-core/src/config.rs:91)) for **every** task in the system — it's a global blanket subscriber, not selectable per task/loop, and returns immediately if that one `chat_id` is unset ([notify.rs:16](crates/lopi-remote/src/telegram/notify.rs:16)). WhatsApp (`whatsapp.rs`) is inbound-only — a Twilio webhook receiver ([whatsapp.rs:40-100](crates/lopi-remote/src/whatsapp.rs:40)) with no outbound-send function at all. No `report`/`report_channel` field exists on `Task`, `LoopConfig`, or `ScheduleEntry` ([config.rs:147-169](crates/lopi-core/src/config.rs:147)). | Add `report: Option<String>` to `ScheduleEntry` ([config.rs:147](crates/lopi-core/src/config.rs:147)) accepting `"telegram"` (WhatsApp has no send path yet, so only Telegram is reachable without new infra). Thread it onto `Task` the same way `autonomy_level` is threaded from `ScheduleEntry` in `scheduler.rs`. Extend `emit_report()` and the `TaskCompleted` handling in `notify.rs` to check this per-task field and send via the *already-instantiated* `Bot`, independent of the global `chat_id` gate. | 0-1 new files (a small `report_sink.rs` in `lopi-remote` if unifying Telegram now / WhatsApp later is wanted; otherwise pure edits). | **Low-Medium** — additive optional field on `ScheduleEntry`/`Task` (serde-default, no break), but wiring crosses `lopi-agent` → `lopi-remote`/`lopi-orchestrator` event-bus boundaries. |
| **4. Skill Arguments** | Frontmatter parser supports exactly `name`, `description`, `user-invocable`, `version`, `triggers`: [skill/lib.rs:97-113](crates/lopi-skill/src/lib.rs:97), field lookups at [lib.rs:167-184](crates/lopi-skill/src/lib.rs:167). Skill body is injected verbatim into the planning prompt: `skill_constraint_blocks()` at [agent/runner/seed.rs:150-160](crates/lopi-agent/src/runner/seed.rs:150) — `format!("Skill «{}» (v{}) — {}\n{}", s.name, s.version, s.description, s.body)`. Matching is trigger-substring only: `relevant_to()` at [skill/registry.rs:63-69](crates/lopi-skill/src/registry.rs:63). | No `$ARGUMENTS` placeholder scanning anywhere in `Skill::body`. No invocation-token grammar (`:kcqf {repo}`) recognized anywhere goals are ingested (CLI, Telegram, webhook). `relevant_to()` only does implicit trigger-matching against the whole goal string — there is no concept of an explicit, addressed invocation with a payload. *(Note: `promote.rs`/`promoter.rs`, present only on `origin/main`, solve a different problem — auto-**creating** new skills from repeated lessons — and do not add argument passing to existing skills.)* | Add `Skill::render_body(&self, args: &str) -> String` doing a literal `self.body.replace("$ARGUMENTS", args)` — reuse the Capability-1 `template::resolve` primitive instead of writing a second substitution routine (DRY). Add a minimal `:<skill-name> <rest>` prefix parser at the goal-ingestion boundary (Telegram `handlers.rs`, CLI) that looks up the skill by name and passes `rest` through as `args`. | 0 new files (one method + one small parse function, ideally colocated with the Capability-1 template module) | **Low** — `Skill` struct needs no new frontmatter field (`$ARGUMENTS` lives in the body markdown, already a `String`); the invocation parser is new code at the ingestion boundary, not a schema change. |

---

## Kill-tests (write these FIRST, before any implementation)

**1. Prompt Templates**
> Assert that submitting a task with `Task::new("test {repo} until {cmd}")` today stores and
> would send the literal, unresolved string `{repo}`/`{cmd}` to Claude — i.e.
> `task.goal.contains("{repo}")` is `true` with no intervening resolution step. This should fail
> once a `template::resolve()` call is inserted ahead of `Task::new` at the producing call sites.

**2. Verifier as Explicit Gate**
> Construct a `Task`/`LoopConfig` with `autonomy_level: DraftPr` (L2) and a
> hypothetical `verifier_required: true` field, run it through pool construction, and assert the
> resulting `AgentRunner` has `verifier_enabled == true`. This fails today for two independent
> reasons that must both be fixed: (a) no such field exists on `Task`/`LoopConfig`, and (b) even if
> it did, nothing in the pool construction path ever calls `.with_verifier()` — `grep` confirms
> zero call sites.

**3. Report on Finish**
> Configure a `ScheduleEntry` with a hypothetical `report = "telegram"` field and **no** global
> `remote.telegram.chat_id`, run a task to completion, and assert a Telegram message was sent.
> Fails today because `notify_loop` returns immediately at [notify.rs:16](crates/lopi-remote/src/telegram/notify.rs:16)
> (`let Some(cid) = chat_id else { return };`) — there is no per-task/per-schedule override path,
> and the field doesn't exist.

**4. Skill Arguments**
> Write a `SKILL.md` whose body contains the literal text `$ARGUMENTS`, invoke it via whatever
> renders it into the planning prompt today (`skill_constraint_blocks`), and assert the literal
> string `$ARGUMENTS` still appears **unresolved** in the constraint block passed to Claude. It
> does, today — there is no substitution step, so this "kill-test" currently passes as a
> demonstration of the gap (it should start *failing*, i.e. the string should resolve, once the
> render step exists).

---

## Recommended Sprint Order

1. **Prompt Templates** (Capability 1) — do first. Zero schema risk, purely additive, and its
   substitution primitive is reused by Sprint 2.
2. **Skill Arguments** (Capability 4) — do second, built directly on Sprint 1's `template::resolve`
   (don't duplicate the substitution logic — same DRY constraint CLAUDE.md's quality framework
   enforces at >85% similarity). No frontmatter/schema change.
3. **Report on Finish** (Capability 3) — **isolate — own sprint.** Adds a field to `ScheduleEntry`
   (config schema) and crosses the `lopi-agent` → `lopi-remote`/`lopi-orchestrator` event-bus
   boundary; keep it separate so a bad interaction doesn't block Sprints 1/2 or get tangled with
   Sprint 4's task-schema change.
4. **Verifier as Explicit Gate** (Capability 2) — **isolate — own sprint,** and do it last. It
   touches *both* `Task` and `LoopConfig` schemas (both have existing round-trip serde tests to
   keep green), *and* it activates `.with_verifier()` for the first time in the pool's history —
   a wiring path that has never executed in production. Highest blast radius of the four; land it
   only after the lower-risk sprints have banked their own lessons.

## Already fully built — skip

**None of the four are complete.** Capability 2 has the most real scaffolding already
shipped and must not be rebuilt: `VerifierVerdict`, the `verifier_verdicts` table, and
`VerifierAgent` (maker/checker isolation, rubric resolution chain, JSON verdict parsing) are all
solid and should be reused as-is — only the "explicit gate" and "differing model/effort" parts are
missing. Everything else (templates, report-on-finish, skill arguments) has no existing
implementation to reuse.
