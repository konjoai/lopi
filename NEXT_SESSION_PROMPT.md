# Next Session Prompts

Paste the relevant entry as the first message of a fresh Claude Code session in
the `lopi` repo. Newest first.

---

## Next Session — after Sprint Successor-1 (Task Lineage and Containment)

**Sprint Successor-1 built the data model, lineage fields, and containment
gates for agent-authored successor tasks — no agent authoring yet.** Read
first, in order: `CLAUDE.md`, `CHANGELOG.md`'s `[0.21.0]` entry, `LEDGER.md`'s
`Sprint Successor-1` entry in full (the three one-way-door decisions:
`SelfAuthored` vs. `SelfModify`, the autonomy-ceiling clamp, and the
untrusted-source ratchet), then this file's own words below.

### What shipped this sprint

- `lopi-core::successor` — the `Successor` proposal type (`goal`/`when`/
  `rationale`/`allowed_dirs`), `SuccessorCondition`, and `Successor::validate()`.
- `Task` gained `parent_task`, `chain_depth`, `successor_enabled`,
  `successor_fixture` (all `#[serde(default)]`); `TaskSource::SelfAuthored`.
  `TaskSource` moved to its own `task_source.rs` (file-size gate).
- `derive_successor_task(parent, successor, max_depth)` — the four
  containment gates (depth cap, autonomy ceiling, directory inheritance,
  untrusted-source lockdown), each with its own dedicated test.
- `AgentEvent::TaskCompleted` gained `successor: Option<TaskId>`.
- `lopi-memory`: `tasks.parent_task`/`tasks.chain_depth` columns +
  `MemoryStore::lineage_chain` (bounded ancestor walk, not a recursive tree).
- `AgentRunner::derive_and_stash_successor` (finalize.rs, beside
  `emit_report`) + pool-level enqueue via the real `AgentPool::submit` —
  gated on `Task::successor_enabled`, fed by `Task::successor_fixture` only
  (no parsing from agent output — that's this sprint's own hard boundary).
- Pre-flight kill-tests KT-A/B/C all recorded (see `LEDGER.md`); 1574
  workspace tests green, clippy clean.

### What could NOT be verified in this sandbox — needs a live check

**The Phase 4 integration test does not drive a real `claude -p` subprocess
through `AgentRunner::run()`'s full plan → implement → test → score loop.**
That requires a live Anthropic API session, which this sandbox cannot reach
(no `claude` CLI session/network for that path). What was actually verified
instead, and why it's still meaningful:
- `crates/lopi-agent/src/runner/finalize.rs`'s `derive_and_stash_successor_*`
  tests prove a passing `finalize()` call really does invoke
  `derive_successor_task` and stash a gated child — the *logic* seam.
- `crates/lopi-orchestrator/tests/successor_enqueue.rs` proves the derived
  child really does land in the real `TaskQueue` via the real
  `AgentPool::submit` (dedup/topology/audit intact) with lineage/depth/gates
  correct on the popped task — the *plumbing* seam.
- **Not yet verified: that a real end-to-end task run (real git repo, real
  `claude -p` session, real diff, real commit) that reaches `TaskStatus::
  Success` actually produces a `TaskCompleted` event with a populated
  `successor` field and a second row appearing in a live `lopi sail`
  dashboard.** This needs a session with real Claude Code CLI access: submit
  a task with `successor_enabled: true` and a `successor_fixture` set (no
  API surface exists yet to set these from the CLI/REST layer — that's
  itself an open question below, KT-1) against a real repo, watch it run to
  completion, and confirm the successor task appears queued and eventually
  dispatched.

### Open questions for Sprint Successor-2

- **KT-1 — no submission surface for `successor_enabled`/`successor_fixture`
  exists yet.** Neither `lopi run`'s CLI flags, the REST `POST /api/tasks`
  handler, nor `.lopi/loop.toml` expose a way to set these fields today —
  this sprint only exercises them via directly-constructed `Task` values in
  tests. Before Sprint Successor-2 adds parsing-from-agent-output, decide
  where a human-supplied fixture successor should be configurable from (a
  repo-level `.lopi/loop.toml` default? a per-task REST field? both?) —
  otherwise the only way to use this sprint's plumbing today is a hand-built
  `Task`.
- **KT-2 — `DEFAULT_MAX_CHAIN_DEPTH = 3` is a hardcoded constant, not a
  per-repo config.** `crates/lopi-core/src/successor.rs` documents this as a
  deliberate scope cut (a natural `.lopi/loop.toml` ceiling once chains
  actually run unattended for a while), but it means every repo currently
  gets the same depth cap regardless of how much it trusts self-extending
  chains. Worth revisiting once Sprint Successor-2/3 make chains something
  that actually runs unattended rather than fixture-only.
- Sprint Successor-2's own explicit scope (per the brief that ran this
  sprint): parse a `Successor` out of an agent's own `final_text`, replacing
  the `successor_fixture` config-only path. Sprint Successor-3: branch
  `advance_to_next_step`'s static-goal chain scheduling on top of dynamic
  successors, plus web/macOS lineage rendering. Sprint Successor-4 (gated on
  a hardware kill-test not yet run): `claude --resume`/`StreamEvent::
  session_id()` — explicitly out of scope until then.

---

## Next Session — after KT-B3-Live

**The attended runbook (`LOPI_KTB3_ATTENDED_RUNBOOK.md`) ran for real for the
first time and did not reach the widget-render question — the server failed
to spawn.** Two independent packaging bugs found and fixed this session, both
verified in one green run. Full detail in `LEDGER.md`'s `KT-B3-Live` entry;
short version below.

Read first, in order: `CLAUDE.md`, `CHANGELOG.md`'s `KT-B3-Live` entry,
`LEDGER.md`'s `KT-B3-Live` entry in full, then `LOPI_KTB3_ATTENDED_RUNBOOK.md`
itself.

### What this session found and fixed

1. **`mcpb/manifest.json` used `${platform}`, which is not a real MCPB
   substitution token** — Claude Desktop's MCP log showed it passed through
   literally, so `entry_point`/`mcp_config.command` resolved to a directory
   that never existed and the server hit "Failed to spawn process: No such
   file or directory" before tool discovery could even start. Fixed by
   hardcoding the literal `server/darwin-arm64/lopi` path (the repo is
   `darwin`-only per `compatibility.platforms`, so no `platform_overrides`
   mechanism was needed).
2. **This branch's `mcpb-release.yml` had regressed to `timeout 10`**
   (unavailable on macOS runners) — a `main`-merge timing gap, unrelated to
   Finding 1. Re-applied `perl -e 'alarm 10; exec @ARGV'` directly.
3. Both verified together in run `29770853385` (headSha `467abb8`), smoke-test
   included — green end to end, real `initialize`/`serverInfo` round trip.

### What a session with real Claude Desktop access needs to do next

1. **Discard the stale `.mcpb` in the repo root** (`lopi-bfe4d7bb...`, the
   artifact from the *failed* attempt) and pull the fresh one:
   `lopi-467abb86e6e3408e73fefc7367db9e72d428587c-darwin-arm64.mcpb` from run
   [`29770853385`](https://github.com/konjoai/lopi/actions/runs/29770853385).
2. **Re-run `LOPI_KTB3_ATTENDED_RUNBOOK.md` from step 1.** This time the
   server should actually spawn — confirm that first (no repeat of the
   `${platform}` failure), then continue: tool list (all eight, including
   `lopi_get_stack_status`), submit/check a real task, watch for an actual
   rendered panel vs. silent text fallback.
3. **Given two packaging bugs slipped past the earlier `mcpb pack`/`unpack`
   verification, don't trust that check alone again** — it exercises the
   bundle mechanics, not the manifest's own command-resolution path a real
   host uses. If this session finds a third packaging issue, that's a sign
   the smoke-test step itself needs to install via a real (or real-ish) host
   path, not just unpack-and-invoke.
4. Write the `LEDGER.md` KT-B3 outcome entry per the runbook's own "either
   way" section — this will be the first time that section has real data to
   report instead of "not attempted."

---

## Next Session — after MCPB-App-1

**The next step is the attended `LOPI_KTB3_ATTENDED_RUNBOOK.md` runbook —
not more Claude Code work.** Everything automatable in `LOPI_DISTRIBUTION_
PLAN.md`'s Track B is now built and packaged. Nothing about the actual
render has been verified — that's not an oversight, it's the correct
boundary a sandbox can't cross, per the runbook itself and the `MCP-Serve-1`
KT2 / `MCP-App-1` KT-D2 precedent for this exact class of blocker.

Read first, in order: `CLAUDE.md`, `CHANGELOG.md`'s `[0.19.0]` entry,
`LEDGER.md`'s `MCPB-App-1` entry in full (the branch-persistence decision,
the join fixture, and — importantly — the macOS-build toolchain finding are
all there), this file's own words below, then `LOPI_KTB3_ATTENDED_RUNBOOK.md`.

### What shipped this sprint (all four deliverables, none render-verified)

1. **Branch persistence** — `tasks.branch`, written by `AgentRunner::
   persist_branch` the moment `TaskStarted` fires. Real column, real store
   call, tested.
2. **`lopi_get_stack_status`** — the eighth MCP tool. Joins the roster with
   per-task DAG stage and branch. Verified against a real two-task,
   two-stage concurrent fixture (KT-B2) — real field values, not just
   success/failure.
3. **The `ui://lopi/stack-status` widget** — `src/mcp_ui/stack_status.html`,
   implements exactly the three lifecycle methods specified
   (`ui/initialize`/`ui/notifications/initialized`/`ui/notifications/
   tool-result`), read-only, no interactivity. Bound via `_meta.ui.
   resourceUri`. `lopi-mcp` gained real `resources/list`/`resources/read`
   support to serve it, plus `structuredContent` on every tool call.
4. **`mcpb/manifest.json` + `.github/workflows/mcpb-release.yml`** —
   `mcpb validate`/`pack`/`unpack` mechanics verified for real (caught and
   fixed two schema errors in the process). **The actual macOS arm64
   binary does not exist yet** — see below.

### A real, concretely-checked blocker this sprint found: no macOS arm64
### binary was produced, and this sandbox structurally cannot produce one

This is new — MCP-App-1 and the plan doc both assumed Deliverable 4 was
sandbox-safe ("nothing here needs nested-spawn access or a GUI host"). That
assumption held for KT-B1/KT-B2 and doesn't hold for a real target binary.
Checked two ways, not assumed (full detail in `LEDGER.md`):

1. Plain `cargo build --target aarch64-apple-darwin` fails immediately —
   this sandbox's `cc` is Linux GCC/Clang, incompatible with `ring`'s
   macOS-targeted build flags.
2. `cargo-zigbuild` gets substantially further (past `ring`, past
   `openssl-sys` with vendored OpenSSL) but hits a hard wall at
   `libgit2-sys`'s own `build.rs`, which unconditionally requires Apple's
   Security.framework/CoreFoundation.framework for any `apple` target —
   no feature flag exists upstream to avoid this. Proprietary Apple
   frameworks aren't obtainable in this sandbox, legitimately or otherwise.

**What a session with real macOS access (attended, or a GitHub Actions run
on the new `macos-14` workflow) needs to do:**

1. Trigger `.github/workflows/mcpb-release.yml` for real (currently
   `workflow_dispatch`-only, deliberately not wired to run automatically
   before its first real run is watched end to end) — or run
   `cargo build --release --target aarch64-apple-darwin --bin lopi` plus
   `mcpb pack mcpb` natively on real Apple Silicon hardware.
2. Confirm the resulting `.mcpb`'s binary actually launches `mcp-serve`
   when invoked exactly as `mcp_config` specifies — the workflow's own
   smoke-test step does this already; if run by hand, replicate it (drive
   a real `initialize` over stdio, confirm `serverInfo` comes back).
3. **Then, and only then**, run `LOPI_KTB3_ATTENDED_RUNBOOK.md` against
   that real bundle: install in Claude Desktop, confirm the tools list
   shows all eight tools including `lopi_get_stack_status`, submit a
   trivial task, and watch whether an actual rendered panel appears versus
   silent text-only fallback. Write the `LEDGER.md` entry for whichever
   outcome happens — both are legitimate, complete results per the
   runbook's own framing.

### Explicitly not started, correctly

Phase B2's remaining items (privacy policy doc, README quick-install
section, desktop-extension form submission) all wait behind KT-B3 clearing,
per the plan's own phasing — not attempted here. One consequence worth
knowing before it surprises anyone: `mcpb/manifest.json`'s
`privacy_policies` array points at `PRIVACY.md`, which doesn't exist in the
repo yet — a 404 until Phase B2 writes it. Sideloading (this sprint's whole
distribution path) doesn't require the file to exist for install to work,
only directory listing does, so this doesn't block anything here — just
don't be surprised the link is dead if you follow it now.

### A repo-doc drift worth fixing — flagged a third time now

`LOPI_DISTRIBUTION_PLAN.md`'s repo copy is still the pre-Track-D-merge
draft (no Deliverables 1–2, no KT-B1/B2/B3, no widget mention in its Track
B section). This sprint, like `MCP-App-1` before it, worked from a pasted
up-to-date version rather than the repo's own stale copy. Third time this
exact drift has been logged (`LEDGER.md`'s `MCP-App-1` and `MCPB-App-1`
entries both flag it) — genuinely overdue for a sync pass; a session that
trusts the repo's own file over a pasted one will miss the entire Track
B/D merge.

---

## Next Session — after MCP-App-1

Read first, in order: `CLAUDE.md`, `CHANGELOG.md`'s `[0.18.0]` entry,
`LEDGER.md`'s `MCP-App-1` entry in full, `LOPI_DISTRIBUTION_PLAN.md`'s
Track D section — **but read it with caution, see the drift note below.**

### What happened this sprint — KT-D2 blocked, correctly, nothing shipped

MCP-App-1 attempted Track D (Loop Stacks inline MCP App dashboard). Its own
hard gate, KT-D2 ("does the MCP Apps `ui/initialize` handshake actually
complete in a real Claude Desktop install and a real claude.ai account"),
cannot be run in this sandboxed environment: headless Linux container, no
`DISPLAY`, no macOS/Windows, no authenticated claude.ai session anywhere on
disk. This was checked concretely (`uname`, `$DISPLAY`, `/Applications`,
credential paths, `ps aux` for any usable interactive `claude` session — see
`LEDGER.md` for the exact commands and output), not assumed. Per the sprint
brief's own instructions, that's a legitimate stop: no widget code, no
`ui://` resource, no new tool implementation were written. This is the
correct outcome, not an incomplete one — don't treat "no widget shipped" as
a task left undone.

**What *was* answered, since it doesn't need live hosts:** KT-D3 (the
tool-binding decision). Full reasoning in `LEDGER.md`, short version: the
widget needs a **new aggregating tool**, not a rebind of
`lopi_get_agent_dag`. Neither existing tool covers Deliverable 4's fields
(task roster + branch + live stage-level `TaskStatus`) — `tasks.status` is
coarse (`"running"` for the entire execution, no stage detail), stage
detail only lives in `agent_dag_nodes`, and **branch has no structured
durable source at all** (only an in-memory event, a freeform log line, or
the terminal `Success{branch}` variant). That last point is a new
prerequisite MCP-App-1 found mid-research, not something the original plan
anticipated: **persisting branch as a real column (or dedicated store call)
when `TaskStarted` fires needs to happen before the aggregating tool can be
built cleanly.**

### What a session with real Claude Desktop and claude.ai access needs to check first

1. **KT-D2 itself.** Build the trivial "hello from lopi" `ui://` resource
   exactly as the original brief specified (a static HTML page, bound to
   any throwaway tool), and attempt the real round trip in a real Claude
   Desktop install and a real claude.ai account. If it renders cleanly,
   proceed to KT-D1. If the handshake fails silently (tool call succeeds,
   resource fetch succeeds, no iframe appears), log the exact protocol
   version / SDK version / host version / failure point and treat Track D
   as blocked pending an upstream fix — there's no client-side workaround
   for a host not completing its half of the handshake.
2. **KT-D1**, once KT-D2 clears: with the trivial resource attached,
   confirm a plain-text MCP-Serve-1 tool (not bound to the resource) still
   renders clean text in Claude Code, nothing broken by the resource's mere
   presence elsewhere in the server.
3. **The branch-persistence prerequisite this sprint found**, before
   building the new aggregating tool: decide how branch gets persisted
   structurally (new `tasks` column vs. a dedicated store call keyed on
   `TaskStarted`) — see `LEDGER.md`'s `MCP-App-1` entry for the exact
   places branch currently does and doesn't appear.
4. Only then: Phase D1 (minimal widget against the new tool), D2 (real
   `structuredContent`), D3 (cross-host verify: Desktop, claude.ai, Cowork
   if reachable; confirm Claude Code still degrades cleanly).

### A repo-doc drift worth fixing before it trips up a future session

`LOPI_DISTRIBUTION_PLAN.md` in the repo is stale — it's the pre-`MCP-Serve-1`
draft (Track A still shown as unbuilt, no Track D section at all). This
sprint's brief pasted an up-to-date version (Track A marked shipped, Track D
added) directly into the session rather than pointing at the repo file,
which is the only reason this sprint had the real Track D spec to work
from. Not this sprint's job to fix (same call as the two-`NEXT_SESSION_
PROMPT.md`-files drift already flagged in the MCP-Serve-1 entry below), but
worth a sync pass — a session that trusts the repo's own copy over a pasted
one will miss Track D's existence entirely.

---

## Next Session — after MCP-Serve-1

Read first, in order: `CLAUDE.md`, `CHANGELOG.md`'s `[0.17.0]` entry,
`LEDGER.md`'s `MCP-Serve-1` entry, `LOPI_DISTRIBUTION_PLAN.md` Track A in full.
Confirm `Cargo.toml`'s version matches `CHANGELOG.md`'s top entry before doing
anything else.

### What landed (MCP-Serve-1, all deliverables 1–5 shipped)

1. `lopi mcp-serve` subcommand (`src/mcp_commands.rs`) — the curated seven-tool
   set from the plan's Track A 1.1 table, over stdio, reusing
   `lopi_mcp::server::serve()` unmodified.
2. `plugin/skills/lopi-cli/SKILL.md` (see the layout note below for why it's
   under `plugin/`, not repo-root `skills/`). Documents the CLI as it ships
   today.
3. `plugin/.claude-plugin/plugin.json` + `.claude-plugin/marketplace.json`
   (repo root) + `plugin/.mcp.json`.
4. Local install verified live: `claude plugin marketplace add`, `claude plugin
   install`, `claude plugin details`, and a real `lopi_submit_task` →
   `lopi_get_task` round-trip through the actual installed/cached binary (not
   just the dev build). `claude plugin validate --strict` clean.
5. Stretch goal (submit to `anthropics/claude-plugins-community`, announce
   publicly) — **not done**, correctly optional per the sprint brief, not
   attempted so as not to rush Phase 2/3.

### Layout deviation from the plan — read before touching plugin files

The plan's package layout puts `.claude-plugin/`, `.mcp.json`, and `skills/` at
the repo root. That fails `claude plugin validate --strict` live: it flags this
repo's own root `CLAUDE.md` as invalid "plugin root" content, and `CLAUDE.md` is
real contributor-facing content that shouldn't move or disappear to satisfy a
plugin validator. The actual layout shipped:

```
lopi/
├── .claude-plugin/marketplace.json    # fixed discovery location — stays here
├── plugin/
│   ├── .claude-plugin/plugin.json     # name: "lopi" — immutable, see LEDGER.md
│   ├── .mcp.json                       # ${CLAUDE_PLUGIN_ROOT}/bin/lopi mcp-serve
│   ├── bin/                            # gitignored — built by scripts/build-plugin-bin.sh
│   └── skills/lopi-cli/SKILL.md
├── scripts/build-plugin-bin.sh
└── src/mcp_commands.rs
```

`marketplace.json`'s one plugin entry has `"source": "./plugin"`. If a future
session touches the manifest layout, re-run `claude plugin validate --strict`
against the actual repo (not a fixture) before trusting any restructure — this
exact failure mode is easy to reintroduce.

### What could NOT be verified this session — needs different access

The Success Criteria's interactive checks — `claude --plugin-dir <path>` loading
the plugin, `/reload-plugins` picking it up, and **the skill actually triggering
on a natural task-submission-shaped prompt** — could not be driven end-to-end
from inside this session. This environment's permission classifier denies a
nested `claude -p`/interactive spawn from within an already-running Claude Code
session (confirmed live during KT2 — the attempt was blocked outright, not
merely slow). What *was* verified as a substitute, and is solid evidence but not
the same thing:

- `claude plugin validate --strict` clean on the real plugin.
- `claude plugin marketplace add` + `claude plugin install` + `claude plugin
  list` + `claude plugin details lopi` all succeed and show the expected
  component inventory (1 skill, 1 MCP server, ~117 tok always-on cost).
- The installed binary at its real cache path (`.../lopi/<version>/bin/lopi`)
  round-trips a real `initialize` → `lopi_submit_task` → `lopi_get_task` MCP
  session correctly.

**What a session with a real interactive `claude` — a human's local machine, or
an environment that doesn't block nested `claude -p` — needs to check:**
`claude --plugin-dir <path-to-lopi-repo>` (or the marketplace-installed
version), then in a live session ask something task-submission-shaped ("submit
a lopi task to fix X") and confirm the `lopi-cli` skill actually fires and picks
the right MCP tool, not just that the skill *would* structurally load. Also
confirm `/reload-plugins` after a manifest edit actually picks up the change
without a full session restart.

### Also flagged, not blocking

`LOPI_VS_OPENCLAW.md`'s feature table (row 2, "Agent Loop") cites an
`AgentState` enum with `Planning → Implementing → Testing → Scoring →
OpeningPr → RollingBack` transitions. The real `AgentState`
(`crates/lopi-core/src/agent.rs`) has no `OpeningPr`/`RollingBack` variants and
is constructed nowhere in the codebase — dead scaffolding. `TaskStatus`
(`crates/lopi-core/src/task.rs`) is the live, CLI/API-surfaced type.
`skills/lopi-cli/SKILL.md` documents `TaskStatus` and calls out the drift
inline; `LOPI_VS_OPENCLAW.md` itself is still stale and worth a small fix
later — not this sprint's job, didn't touch it.

### Explicitly not started (non-goals, correctly)

Track B (MCPB desktop extension) and Track C (Connectors Directory) — neither
touched. Track B reuses the exact same `ToolHandler` and state-sharing design
(see `LEDGER.md`); Track C needs its own re-derivation, not a copy-paste of
KT4's answer (a Streamable HTTP transport serving multiple concurrent clients
changes the dispatch-ownership calculus — see `LEDGER.md`'s closing note).
