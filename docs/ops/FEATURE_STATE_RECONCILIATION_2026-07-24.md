---
decays: historical
---

# Feature State Reconciliation — 2026-07-24

**Baseline:** `main` @ `63908a5`, v0.24.0 · **Date:** 2026-07-24
**Method:** structural / source-level check only — NOT a live audit. No authenticated
`lopi sail` run, no real billed agent runs, no screenshots. Each finding below is
verified by reading the current implementation and, where one exists, an
accompanying test — the same standard `docs/ops/FEATURE_STATE_FINAL.md` itself
distinguished as weaker than its live-verified rows. Where source alone cannot
settle a claim, that is stated explicitly as "needs live proof," not guessed.
**Reconciles:** `docs/ops/FEATURE_STATE_FINAL.md` (Verify-1, `a6e4b5f`, v0.3.2,
2026-07-10) findings F1–F8. F5 was not present in that doc's findings index and
is not covered here.

---

## Verdicts

| Finding | Verdict | Citation |
|---|---|---|
| F1 | **Fixed** | `src/util.rs:66-88` |
| F2 | **Fixed** (one caveat, see below) | `web/src/lib/stores/stackRun.ts:450-519` |
| F3 | **Fixed** | `crates/lopi-ui/src/web/handlers.rs:25-48` |
| F4 | **Fixed** | `web/src/routes/overview/+page.svelte:40` |
| F6 | **Fixed** | `web/src/lib/stores/agents.ts:229-232` |
| F7 | **Fixed** | `crates/lopi-core/src/tier.rs` (Growth `features()`, ~78-85) |
| F8 | **Fixed** | `crates/lopi-memory/src/store/mod.rs:349`, `crates/lopi-ui/src/web/task_stream_handlers.rs:56-102`, `crates/lopi-ui/src/web/metrics_handlers.rs:57-74` |

**All seven findings are fixed at the source level.** Each fix carries an
in-code comment tracing back to its original finding ID, and `CHANGELOG.md`'s
v0.3.3 entry ("Fix-2") corroborates the round as deliberate, not incidental.

---

## Detail

**F1 — `--config` silently swallowing a partial TOML.** `src/util.rs:66-88`
(`load_config`): a `--config` path that fails to parse now hits `LopiConfig::load`'s
`Err` branch and emits `tracing::warn!("failed to load --config {}: {e:#} —
falling back to default config/DB", ...)` before returning `None`. Behavior still
degrades to defaults, but the "no silent failures" rule is satisfied — the
operator gets a named, loud warning. **Fixed.**

**F2 — bare pane had no launch control.** `web/src/lib/stores/stackRun.ts:450-477`
(`runBarePane`) and `:482-519` (`launchBareCard`) give `paneSubmitPayload`
(`web/src/lib/stores/stack.ts:1548`) a real caller — it builds the payload and
calls `createTask`, wiring `taskId`/terminal status onto the card. **Fixed at the
store layer.** Caveat: this reconciliation did not trace the `StackPane.svelte`
markup to confirm a visible run control invokes `runBarePane` — that link is
inferred from the store wiring and the CHANGELOG entry, not directly grepped.
If a live UI pass is ever run, that's the one thing worth re-confirming for F2
specifically.

**F3 — `/api/stats` state counters wrong.** `crates/lopi-ui/src/web/handlers.rs:25-48`
(`get_stats`) now sources counts from `s.store.status_counts()` — a durable DB
query (`crates/lopi-memory/src/store/mod.rs:320`) — instead of the old
per-pool in-memory `pool.stats()`, which missed multi-repo tasks. **Fixed.**

**F4 — topbar "N live" undercount.** The standalone topbar live-counter from the
baseline audit no longer exists as a distinct element (`web/src/routes/+layout.svelte:42-87`
has no live-count display). The surviving live count lives only at
`web/src/routes/overview/+page.svelte:40`, sourced from the all-repo client-side
`agents` map — the same source the original audit already validated as correct
for `/overview`. No code path reproduces the old undercount. **Fixed** (by
consolidation onto the already-correct source, per the nav collapse documented
in `web/src/lib/stores/nav.ts:21-27`).

**F6 — client-store cost surfaces showing $0.** `web/src/lib/stores/agents.ts:229-232`
hydrates `cost: t.cost ?? 0` from the initial task snapshot (comment cites
"Verify-1 F6"); `web/src/lib/parser.ts:412-415` preserves `cost` through
`parseWireMessage`. `budget.ts:76` and the overview cost rows now read this
hydrated value, matching server truth. **Fixed.**

**F7 — `tier.rs:81` advertising a cut feature.** The current Growth tier
`features()` list no longer contains "Constellation routing (4 strategies)" or
any "strategies" bullet — it lists concurrency, multi-repo dispatch, caching,
tool registry, OTel, and priority support instead. `CHANGELOG.md` v0.3.3
confirms deliberate removal. **Fixed.**

**F8 — bogus task-id returning 200 instead of 404.** All three routes now gate
on `store.task_exists` (`crates/lopi-memory/src/store/mod.rs:349`, with its own
test distinguishing known from bogus IDs): `/api/tasks/:id/stream`
(`task_stream_handlers.rs:56-63`), `/api/tasks/:id/logs` (`task_stream_handlers.rs:95-102`),
and `/api/agents/:id/dag` (`metrics_handlers.rs:57-74`) all 404 on an unknown ID.
**Fixed.**

---

## What this reconciliation is not

This is a source-level pass, not the live, real-auth, real-spend re-verification
`docs/ops/FEATURE_STATE_FINAL.md` originally performed. It confirms the *code*
that caused each finding has changed in the direction the finding demanded, and
each change carries a test or an explicit trace to the finding ID. It does not
re-observe the behavior live in a running `lopi sail` the way Verify-1 did. A
full F1–F8 live re-verification (screenshots, real network traces, real billed
runs) remains out of scope for this sprint — see `NEXT_SESSION_PROMPT.md`.
