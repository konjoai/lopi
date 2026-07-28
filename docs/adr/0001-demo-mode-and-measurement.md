# ADR 0001: `lopi demo` and the Honest Measurement Policy

**Status:** Accepted
**Sprint:** demo-measurement

## Context

Two independently-shippable features, landed together because they share one
surface — how lopi labels the numbers it shows:

- **Part A**: `lopi demo` — a subcommand that fabricates a complete,
  self-consistent lopi store (repos, tasks, agent traffic, memory tables) so
  someone can see a fully alive dashboard with zero setup.
- **Part B**: a public measurement policy (`docs/MEASUREMENT.md`) plus a
  `Provenance` type attached to every user-facing metric, so a number never
  implies more certainty than lopi actually has.

Per the sprint brief, the two ideas were identified by analyzing a
third-party PolyForm-Noncommercial-licensed project. No code from that
project was fetched, read, or reproduced — everything here is implemented
from the abstract spec and this repo's own conventions.

## What the spec assumed that turned out to be wrong

A pre-implementation research pass (`Explore` agent over
`crates/lopi-memory`, `crates/lopi-remote`, `crates/lopi-ui`,
`crates/lopi-mcp`, `src/`) surfaced several stale assumptions in the sprint
brief:

1. **There is no `/cost` Telegram command to fix.** The Telegram bot
   transport (`crates/lopi-remote/src/telegram.rs`) was fully removed in
   Sprint S10, Phase 4 — see `crates/lopi-remote/src/lib.rs`'s doc comment
   and `LEDGER.md`. `TaskSource::Telegram` survives only as an enum variant
   for historical row deserialization; `[remote.telegram]` in `LopiConfig`
   is a dead config struct nothing reads at runtime. `lopi-remote` today
   contains only `whatsapp.rs` — an inbound-only Twilio webhook that enqueues
   a `Task` and always replies with empty TwiML. It has no command parser and
   no outbound-reply capability at all, so "Telegram answers queries and
   refuses mutations" (A.5) has no live surface to refuse from.

   **Resolution:** Part B's "fix `/cost` specifically" is retargeted at the
   cost/usage surfaces that actually exist and are most visible today:
   `GET /api/stats`'s `total_cost_usd_today`/`total_tokens_today`, the
   `GET /api/budget/breakdown` endpoint, the `lopi run` / loop-runner
   "💵 session cost" stdout line, and the REPL's session-cost status line.
   Each now carries an explicit label plus a `Provenance` value.

2. **There is no `dead_letter.rs` / `DeadLetterRow`.** Dead-lettering is
   represented as an `audit_log` row with `action = "task.dead_letter"`
   (`crates/lopi-memory/src/store/audit.rs`), not a dedicated table. The demo
   generator's "dead-letter entries with real-looking blockers" (A.7) are
   `audit_log` rows with that action and a JSON payload describing the
   blocker, matching how a real dead-letter is recorded today.

3. **There is no `Blocked` `TaskStatus` variant.** The enum
   (`crates/lopi-core/src/task.rs`) covers `Queued, Planning,
   AwaitingPlanApproval, Implementing, Testing, Scoring, Retrying, Success,
   Failed, RolledBack, Conflict` — no `Blocked`. The demo generator covers
   every variant that exists; "blocked" reads as a `Failed` task paired with
   a `task.dead_letter` audit entry, which is exactly how a real blocked
   goal shows up today.

4. **"Repos" are not a table.** The real dashboard discovers repos by
   scanning the filesystem for sibling `.git` directories
   (`crates/lopi-ui/src/web/repos_handlers.rs::scan_repos`) — there is no
   `repos` table in `schema.sql`. Demo mode must never touch the real
   filesystem (A.1: "no environment inspection, no git calls"), so it can't
   reuse that path. A new `demo_repos` table holds the four synthetic repo
   descriptors, and `repos_handlers`/the branches/claude-commands endpoints
   now short-circuit to synthetic data (or an empty list) whenever the open
   store reports `is_synthetic() == true`, instead of scanning.

5. **An existing `provenance` field already exists on the wire, with a
   different meaning.** `TaskRow::provenance()`
   (`crates/lopi-memory/src/store/task_row.rs`) returns
   `"operator"|"untrusted"|"unknown"` — a *trust* classification for the
   plan-approval gate (is this task's `source` an authenticated human/CLI
   path or an unauthenticated webhook?), already serialized as `"provenance"`
   in `GET /api/tasks`. Part B's `Provenance` enum is a *measurement
   confidence* classification and must not collide with that JSON key.
   Every new provenance-carrying field added to a JSON response uses the key
   `"measurement_provenance"`.

6. **No `dirs`/`directories` crate, no hoisted `rand`.** XDG/home resolution
   is hand-rolled everywhere via `$HOME` (`src/util.rs::db_path`,
   `crates/lopi-core/src/config.rs::find_and_load`). The demo store path
   follows the same convention — `~/.lopi/demo.db`, a sibling of
   `~/.lopi/lopi.db` — rather than introducing a new dependency for an XDG
   data dir the rest of the codebase doesn't use. `rand` existed only in
   `lopi-agent`'s own `Cargo.toml` (not hoisted); it is now hoisted to
   `[workspace.dependencies]` since both `lopi-demo` and `lopi-agent` need
   the same seeded-RNG primitive.

7. **The TUI is purely event-bus driven — it never reads the store.**
   `lopi_ui::tui::run` builds an empty `AppState` and only populates
   `agents`/logs from live `AgentEvent`s received on its bus
   (`crates/lopi-ui/src/tui.rs`). The web dashboard, by contrast, reads the
   store directly on every request and on WS-connect
   (`handlers::list_tasks`, `streaming::handle_ws`'s snapshot). So:
   - The **web dashboard needs no event replay at all** — it is "fully
     alive" the moment the demo store has rows, exactly like a real
     dashboard opened against a store with history.
   - The **TUI needs a one-time seed** of synthetic `AgentEvent`s so its
     event-driven state populates without a live agent pool. `tui::run` grew
     an additive `run_with_seed(bus, initial_events, synthetic)` entry point
     (the existing `run(bus)` is now a thin wrapper calling it with an empty
     seed) that folds `initial_events` into `AppState` before the live loop
     starts, avoiding the subscribe-timing race a bus `send()` before
     `subscribe()` would hit.

## Decisions

- **Demo store path**: `~/.lopi/demo.db` (mirrors `~/.lopi/lopi.db`'s own
  convention — see point 6 above), not an XDG data dir.
- **Generator crate**: `crates/lopi-demo`, a library crate depended on by
  the `lopi` binary (`src/demo_commands.rs`) and by test suites, per A.1's
  "not buried in the CLI" requirement. Depends on `lopi-core` + `lopi-memory`
  only.
- **Isolation guard**: `lopi_demo::generate` canonicalizes both the
  destination and the caller-supplied real-store path (falling back to
  parent-dir canonicalization + filename join when the destination doesn't
  exist yet) and refuses with `anyhow::bail!` if they resolve to the same
  file. Tested directly, and tested again by pointing `lopi.toml`'s
  `db_path` at the default demo path and asserting the CLI command refuses.
- **Synthetic marker**: `store_metadata` key/value table (generic — not
  demo-specific machinery, could serve other single-row config later),
  written only by the generator (`synthetic=true`, `demo_seed=<seed>`,
  `demo_generated_at=<rfc3339>`). Every surface's synthetic badge reads
  `MemoryStore::is_synthetic()`, which is a store query, not a CLI flag —
  opening a demo store through any code path (even one that forgot to pass
  `--demo`) still announces itself, satisfying A.4's "driven by the store
  marker" requirement.
- **Determinism**: content (goal/task text, repo identities, pattern/lesson
  text, counts) is drawn from a `rand::rngs::StdRng::seed_from_u64(seed)`.
  Timestamps use real `Utc::now()` at generation time (offsets between rows
  are seeded/deterministic; the anchor is not), per A.6.
- **Default seed**: `lopi demo` with no `--seed` uses a fixed constant
  (`1337`), not a random seed — this keeps a bare `lopi demo` reproducible
  run-over-run (matching the spirit of "generate if absent") without forcing
  every screenshot-taker to remember a flag.
- **Refusal-in-depth**: demo mode's `lopi_ui::web::serve_with_repo`-equivalent
  path never starts the scheduler/chain-scheduler/quota-tracker/MAXX loop and
  never spawns the agent pool's dispatch loop — so even if a mutation
  endpoint accepted a write, nothing would ever consume it (no git call, no
  `claude` spawn is reachable). On top of that structural guard, the task
  mutation endpoints most directly tied to "act on a task" (`create_task`,
  `cancel_task`, `approve_plan`, `reject_plan`) explicitly check
  `is_synthetic()` and return `403` — belt and suspenders, per the brief's
  "wire the guards, don't rely on convention."
- **Grep-and-fix sweep findings**: see `docs/MEASUREMENT.md`'s own section
  linking back here. The full-workspace inventory (Rust: pricing/budget/cost
  fields across `lopi-core`, `lopi-memory`, `lopi-agent`, `lopi-ratelimit`,
  `lopi-ui`; the SvelteKit frontend's independent dollar-formatting in
  ~20 files under `web/src/`) is larger than one sprint can carry to
  100% provenance coverage. This sprint attaches `Provenance` to the
  highest-traffic surfaces (`/api/stats`, `/api/budget/breakdown`, the
  `lopi run`/loop-runner cost line, the REPL cost line, the pricing table's
  staleness warning) and leaves the SvelteKit dashboard's own cost displays
  and the lower-traffic budget/loop-health JSON fields as documented,
  tracked debt — each site is enumerated in `docs/MEASUREMENT.md` under
  "Known gaps," per this repo's `KNOWN DEBT` convention
  (`.konjo/scripts/soft_gate_lint.py`), rather than silently left unlabeled.

## Consequences

- `MemoryStore::is_synthetic()` becomes a cheap, always-available check any
  new surface should call before rendering a number as if it were real.
- The web dashboard's demo-mode behavior falls out of existing
  store-is-the-source-of-truth architecture almost for free; the TUI needed
  one additive, backward-compatible signature change.
- The `Provenance` type's JSON field name (`measurement_provenance`) is now
  a naming convention every future metric-carrying endpoint should follow to
  avoid colliding with `TaskRow::provenance()`'s existing trust field.
