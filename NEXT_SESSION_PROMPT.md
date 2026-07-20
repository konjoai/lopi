# Next Session Prompts

Paste the relevant entry as the first message of a fresh Claude Code session in
the `lopi` repo. Newest first.

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
