# Ledger

A running log of load-bearing design decisions — the ones that would be
expensive to silently re-litigate in a later sprint. One entry per sprint,
newest first. Not a changelog (that's `CHANGELOG.md`) — this is *why*, not
*what*.

## Sprint 4 — Verifier as Explicit Gate (`crates/lopi-agent/src/verifier.rs`, `crates/lopi-agent/src/runner/verifier_runner.rs`, `crates/lopi-core/src/{loop_config.rs,task.rs}`, `crates/lopi-orchestrator/src/pool/run_loop.rs`)

**Decision (never-grade-your-own-homework default):** when `verifier_model` is
unset, the resolved verifier model must differ from the worker model that
produced the diff being graded. Documented default: **Opus**, unless the
worker itself already ran on Opus (an escalated retry, `attempt >= 2` per
`select_model`), in which case the verifier falls back to **Sonnet** instead.
This is a pure function, `lopi_agent::verifier::resolve_verifier(worker_model,
verifier_model, verifier_effort) -> (model, effort)`, unit-tested in isolation
— it is the one place this rule is enforced, so `run_verifier_pass` never
duplicates the logic. An *explicit* `verifier_model` is always honored as-is,
even if it happens to equal the worker's model — that's a deliberate operator
override, not a default, and enforcing "different" there would silently
override a user's stated choice.

**Decision (effort is a prompt hint, not a wire parameter):** `verifier_effort`
threads into `VerifierAgent::verify`'s system prompt as a plain-text
`"Reasoning effort: {effort}"` line, the same convention the web cockpit
already uses for worker-side launch controls (`web/src/lib/stores/agents.ts`
folds its `effort` selector into a planning constraint the same way — see
`CHANGELOG.md`'s "Model / effort / priority / repo / branch selectors" entry).
The Anthropic API client (`AnthropicClient::complete`) has no reasoning-effort
request parameter at all — only a token-based `task_budget` (Phase 16.6),
which is a different mechanism (self-pacing, not reasoning depth). Inventing a
wire-level parameter that doesn't exist would be scope creep beyond "activate
and parameterize" the existing VerifierAgent; folding it into the system
prompt text reuses an established pattern instead of adding a new one.

**Decision (the pool-construction seam):** `run_one`'s runner-builder chain
was extracted into `build_runner` — a pure assembly function (no I/O) that
takes every already-resolved input and returns the configured `AgentRunner`,
calling `.with_verifier()` when `task.verifier_required ||
task.verifier_model.is_some()`. This is the load-bearing kill-test seam
(Capability 2's kill-test, `PROMPTS_PLAN.md`): a unit test builds a `Task`
with `verifier_required = true` and an `AutonomyLevel::DraftPr` (L2, which
alone would *not* force the verifier) and asserts the resulting
`AgentRunner::verifier_enabled()` is `true` — without ever calling `.run()`,
so the never-before-exercised maker/checker flow is proven wired without
actually executing it. `AgentRunner::verifier_enabled()` (a `pub const fn`
getter) was added for exactly this assertion; the field itself
(`AgentRunner.verifier_enabled`) already existed but had no external reader.

**Why the seam, not a network-level assertion:** `PROMPTS_PLAN.md`'s literal
kill-test wording ("assert the client received SONNET, not OPUS") implies
intercepting the outbound HTTP call, but `AnthropicClient` has no
base-URL injection point and the workspace has no HTTP-mocking dependency.
Adding one would be a new third-party dependency and a wire-level change to
`AnthropicClient` — both outside this sprint's pre-authorized scope ("REUSE
[VerifierAgent] AS-IS... this sprint only activates and parameterizes it").
The equivalent, dependency-free proof: `resolve_verifier` (the only place a
model gets chosen) is unit-tested directly, and `verify`'s body — visible in
the diff this sprint prints — has zero remaining reference to a hardcoded
model constant; the `model: &str` parameter flows straight into `.complete()`
with no branch in between.

**What now exercises the previously-dead `.with_verifier()` path:** any task
or `.lopi/loop.toml` that sets `verifier_required = true` or a
`verifier_model`, submitted through `AgentPool::submit` → `run_one` →
`build_runner`. Before this sprint the only way to force the verifier was
`autonomy_level >= VerifiedPr` (L3/L4); that mechanism is untouched
(`requires_verifier` in `finalize.rs` still ORs both together at finalize
time). The first time this call site runs in production will be the first
real, live exercise of `VerifierAgent`'s maker/checker isolation outside its
own unit tests — treat an early failure there as expected discovery, not a
regression.

**Housekeeping:** two existing test-only `Task { .. }` struct literals
(`crates/lopi-agent/src/runner/api_plan.rs`, `crates/lopi-agent/src/stability/mod.rs`)
needed the three new fields added to compile; `dry_check.py` still flags
these two helpers as near-duplicates of each other (pre-existing, unrelated
to this sprint — both already duplicated the full `Task` literal before this
change) and unrelated pre-existing duplication elsewhere in the workspace
(`lopi-webhook`, `lopi-spec`, `lopi-remote`). No verifier logic itself is
duplicated anywhere — `resolve_verifier` and the one `.with_verifier()` call
site are each defined exactly once.

**How to apply:** any future "gate" field that should be forceable
independent of `autonomy_level` should follow this same shape — a bool +
optional override(s) on both `LoopConfig` and `Task`, `#[serde(default)]`,
read at the pool-construction seam rather than threaded through `.lopi/loop.toml`
at runtime (Task is the authoritative per-run source, matching how
`autonomy_level` already works — `LoopConfig`'s copy is the UI-editable
repo-level default/display value, not something `run_one` re-reads
automatically). Any future "resolve a value that must differ from another
value" pattern should follow `resolve_verifier`'s shape: a pure function,
unit-tested in isolation, called from exactly one production site.

## Sprint 3 — Report on Finish (`crates/lopi-core/src/{report.rs,config.rs,task.rs,event.rs}`, `crates/lopi-agent/src/runner/finalize.rs`, `crates/lopi-remote/src/telegram/notify.rs`)

**Decision (dependency edge):** neither pre-authorized edge (`lopi-agent` →
`lopi-remote`, or a trait-in-core) was taken. Reading the actual dep graph
first showed `lopi-remote` already depends on `lopi-orchestrator`, which
depends on `lopi-agent` — so `lopi-agent` → `lopi-remote` would have been a
real cycle, exactly the failure mode `NEXT.md` flagged up front. Instead,
`AgentEvent` (already in `lopi-core`, already depended on directly by both
`lopi-agent` and `lopi-remote`) gained one new variant, `ReportReady { task_id,
channel, summary }`. `emit_report` broadcasts it on the existing
`EventBus<AgentEvent>`; `lopi-remote`'s already-running `notify_loop` gained
one new match arm that calls the existing `send_msg` helper. Net new
dependency edges: **zero** — `cargo tree -p lopi-agent` / `-p lopi-remote`
are unchanged, no `Cargo.toml`/`Cargo.lock` edits at all. This is a stronger
fit than either pre-authorized option: it needed no new abstraction (the
event-bus *is* the report-sink seam) and no cross-crate call.

**Decision (chat_id):** option (a) — the report reuses the single global
`remote.telegram.chat_id` this loop was booted with. `notify_loop`'s existing
gate (`return` when `chat_id` is `None`) is untouched; `ReportReady` just adds
another event the existing `chat_id: ChatId` in scope can be sent to. **Known
limitation:** every `report = "telegram"` schedule in a given `lopi` process
notifies the same chat — there is no per-task destination yet. Building
per-task routing (option b — `ScheduleEntry` carrying a target chat id) was
explicitly out of scope this sprint (`NEXT.md`: "do NOT build a full per-task
routing system"); revisit if/when multiple distinct Telegram destinations are
needed.

**Decision (channel validation):** `report: Option<String>` (not a typed enum
field) on both `ScheduleEntry` and `Task`, per `NEXT.md`'s explicit call —
threaded from `ScheduleEntry` to `Task` in `scheduler.rs` the same one line as
`autonomy_level`. The typed side is `ReportChannel::parse(&str)` in the new
`lopi-core::report` module: `"telegram"` parses; `"whatsapp"` is a *named*
`WhatsappUnsupported` error (inbound-only Twilio webhook, no send path — not
lumped in with generic `Unknown`); anything else is `Unknown(name)`. Called
in two places, both reusing the same `parse` fn (no second scanner): (1)
`LopiConfig::load()` validates every `[[schedules]]` entry's `report` and
fails the whole load loudly on a bad channel — a typo'd config never silently
never-sends; (2) `emit_report` re-validates defensively (a `Task` can reach
`emit_report` from sources other than `ScheduleEntry`), `tracing::warn!`-ing
and skipping the broadcast rather than sending an unrecognized channel name.

**Why:** the config-load validation is the one guaranteed choke point — every
`ScheduleEntry` a user writes passes through it, so it is where a typo must be
caught, not where it's merely convenient to catch it. Re-validating at
`emit_report` costs one extra `match` and closes the gap for tasks built
outside the schedule path (API, CLI) that could carry an unvalidated `report`
string directly.

**Housekeeping:** `crates/lopi-core/src/event.rs` was already at 590 lines
(over the 500-line hard gate) before this sprint; adding `ReportReady` pushed
it to 621. Since the file-size CI gate scans *changed* files on a PR, this
sprint's edit would have tripped it. Split the file's two `#[cfg(test)]`
modules out to `event_tests.rs` / `event_wire_format_tests.rs` via the
`#[path = "..."]` pattern already used by `config_tests.rs` /
`loop_config_tests.rs` — a pure test-relocation, zero logic changes — bringing
`event.rs` itself to 323 lines. Same category of proactive split as
`run_loop.rs`'s (Sprint 2 era), just triggered by an existing-debt file this
time rather than new code.

**How to apply:** any future `lopi-agent` → `lopi-remote` (or similarly
"downstream" crate) communication should default to an `EventBus<AgentEvent>`
variant before reaching for a new dependency edge or a bespoke trait —
check `cargo tree` for the real graph first, since a plausible-looking direct
call can be a cycle in disguise. Any new `report`/channel-shaped field should
validate through `ReportChannel::parse`, not a second name-matching branch.

## Sprint 2 — Skill Arguments (`crates/lopi-skill/src/{lib.rs,invocation.rs}`)

**Decision:** empty `args` on a body containing `$ARGUMENTS` is an **empty
fill, not an error** — `$ARGUMENTS` becomes `""`, and rendering still
succeeds. And: `render_body` reuses `template::resolve` by *translating*
`$ARGUMENTS` → `{arguments}` and calling `resolve` with a one-entry
`{"arguments": args}` vars map — no second `.replace()`/scanner, per Sprint
1's hard reuse constraint. `Skill` needs no new frontmatter field for this;
`$ARGUMENTS` lives in the existing body `String`.

**Why:** an empty-fill (not an error) is the least-surprising choice —
`:kcqf` alone (no argument) is a legitimate, common invocation shape, and
`resolve` itself already treats a *present* vars entry mapped to `""` as a
perfectly valid substitution (this is distinct from a *missing* key, which
is still the loud `TemplateError` Sprint 1 built). Erroring on empty args
would penalize the common case for no real safety gain. On reuse: the
translate-then-delegate approach was chosen over extending `resolve` with a
second hole syntax (`$NAME`) because it needed **zero changes** to
`template.rs` — the smallest change that could possibly work, and it
composes: any future skill-body placeholder can follow the same
translate-to-`{hole}` pattern without `template.rs` ever learning a second
syntax. The tradeoff this creates: a skill body with a genuinely stray,
unescaped `{` (not part of `$ARGUMENTS`) will error on invocation, exactly
as a hand-written template would — skill authors get Sprint 1's `{{`/`}}`
escape rule "for free," not a more lenient bespoke rule.

**How to apply:** any future skill-body placeholder should translate to a
`{hole}` and delegate to `resolve`, not add new substitution logic. If a
skill body needs to contain a literal, un-doubled `{` going forward, that's
now a real authoring constraint worth documenting in the skill-writing docs,
not a bug in `render_body`.

## Sprint 1 — Prompt Templates (`crates/lopi-core/src/template.rs`)

**Decision:** escaping follows Rust's `format!` rule — `{{` and `}}` decode to
a literal `{` / `}`, independently of hole-matching (not a paired
`{{...}}` block). And: stop at a bare `resolve()` fn — no `PromptTemplate`
newtype.

**Why:** the escape rule is copied wholesale from a convention every
Rust contributor to this repo already knows (`format!`/`println!`), so there's
no new grammar to learn or document — `{{brace}}` reads as "the same rule as
`format!`" instead of a bespoke invention. The fn-vs-newtype call: a newtype
would only earn its keep once templates carry state beyond the string itself
(a source location, a cached parse, validation metadata) — none of which this
sprint's four call sites need. Building it now would be exactly the kind of
premature abstraction CLAUDE.md warns against; the moment a second sprint
needs more than a `&str` in, `String` (or `Result`) out, promote it then.

**How to apply:** any future sprint that touches template syntax (nested
holes, default values, conditional holes) must extend this same escape rule
rather than introducing a second one — and should re-examine the newtype
question at that point, not before.
