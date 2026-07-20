# lopi Public Distribution Plan

**Status:** Draft for review
**Scope:** Claude Code plugin (Track A), MCPB desktop extension (Track B), Connectors Directory (Track C — deferred, research only)
**Author context:** Konjo AI / lopi (github.com/konjoai/lopi)

---

## 0. Decision summary

Three tracks, sequenced by friction, not by ambition:

| Track | Gate | Status this doc |
|---|---|---|
| A — Claude Code plugin | None (self-hosted repo) | Full spec + GTM + roadmap + timeline |
| B — MCPB desktop extension | None to sideload; submission form to get listed | Full spec + GTM + roadmap + timeline |
| C — Connectors Directory | Team/Enterprise org to submit; none to build | Research only — build once traction from A/B justifies the Enterprise seat |

**Grounded in the actual repo, not the README.** Two facts drive both Track A and Track B:

1. `crates/lopi-mcp/src/lib.rs:1-36` — a full MCP crate already exists: JSON-RPC envelope, protocol messages, a stdio client, and a server side (`server.rs`) with a `ToolHandler` trait and a transport-agnostic `serve()` loop (`crates/lopi-mcp/src/server.rs:17-54`). It's in the workspace (`Cargo.toml:19`) and compiles. But `grep -rln "lopi_mcp" crates/*/src src/` returns nothing — zero call sites. No `mcp` subcommand in `cli.rs`. **This is unwired scaffolding**, not a shipped feature.
2. `crates/lopi-ui/src/web/mod.rs:198-317` — 30+ live REST routes (`create_task`, `list_tasks`, `cancel_task`, `get_agent_dag`, `get_logs`, `get_quota`, etc.), served by `lopi_app::serve`, which **is** wired into `src/main.rs:187`.

Both tracks need the same underlying engineering: a curated MCP tool layer over lopi's task/agent operations. Track A ships it as `.mcp.json` inside a plugin; Track B ships the same binary as a `.mcpb`. **This is one engineering dependency, not two.** Build it once, package it twice.

**Non-goals for this phase:**
- The official Anthropic-curated plugin tier (`claude-plugins-official`) — invite/high-bar, not a submission queue. Not targeting it directly; if the community-marketplace listing gets traction, Anthropic can pull it up-tier on their own initiative.
- The Skills Directory partner tier — brand-partnership only (Notion/Canva/Figma/Atlassian), not self-serve. A `skills/` folder ships inside the plugin instead (see Track A spec).
- Code-signing spend (Apple Developer ID, Windows cert) before sideload traction validates there's demand worth paying for.
- Any Connectors Directory build work until an Enterprise org exists to submit through.
- Bundling the `;` composer grammar redesign into either track's v1 — that sprint isn't landed yet. v1 skills document the CLI as it exists today; the grammar becomes a v1.1 skill update once it ships.

---

## TRACK A — Claude Code Plugin

### 1.1 Spec

**Package layout** (self-hosted at `konjoai/lopi`, or a dedicated `konjoai/lopi-plugin` if you don't want plugin scaffolding cluttering the main repo root):

```
lopi-plugin/
├── .claude-plugin/
│   ├── plugin.json          # plugin manifest
│   └── marketplace.json     # marketplace catalog (if self-hosting from this repo)
├── skills/
│   └── lopi-cli/
│       └── SKILL.md         # teaches Claude the lopi CLI + output formats
├── .mcp.json                 # points at the lopi mcp-serve binary
└── README.md
```

**`plugin.json`** — only `name` is strictly required, but the marketplace listing wants the full set:

```json
{
  "name": "lopi",
  "version": "0.1.0",
  "description": "Multi-agent Claude Code orchestrator — submit tasks, watch agents run, check status from inside a Claude Code session.",
  "author": { "name": "KonjoAI", "url": "https://github.com/konjoai" },
  "homepage": "https://github.com/konjoai/lopi",
  "repository": "https://github.com/konjoai/lopi",
  "license": "MIT",
  "keywords": ["agent-orchestrator", "multi-agent", "rust", "claude-code"]
}
```

`name` is an **immutable slug once published to any marketplace** — decide `lopi` vs `lopi-orchestrator` now, not after the community marketplace has it pinned to a commit SHA.

**`.mcp.json`** — points at a new subcommand, not the unwired library crate directly:

```json
{
  "mcpServers": {
    "lopi": {
      "command": "${CLAUDE_PLUGIN_ROOT}/bin/lopi",
      "args": ["mcp-serve"]
    }
  }
}
```

This means Phase 1 engineering work (below) is a real prerequisite, not optional polish: **`lopi mcp-serve` doesn't exist yet.** It needs to be a new subcommand in `src/cli.rs` that constructs a `ToolHandler` impl calling into the same state `lopi_app::serve` already uses, and runs `lopi_mcp::server::serve()` over stdio.

**Curated tool set (v1, keep it under ~10 tools — connector token-limit guidance applies here too):**

| Tool | Wraps existing route |
|---|---|
| `lopi_submit_task` | `POST /api/tasks` |
| `lopi_list_tasks` | `GET /api/tasks` |
| `lopi_get_task` | `GET /api/tasks/:id` |
| `lopi_cancel_task` | `DELETE /api/tasks/:id` |
| `lopi_get_logs` | `GET /api/tasks/:id/logs` |
| `lopi_list_agents` / `get_agent_dag` | `GET /api/agents/:id/dag` |
| `lopi_get_stats` | `GET /api/stats` |

Everything else (schedules, quota, webhook config) stays out of v1 — it's admin surface, not agent-facing, and every tool added is context budget spent on every turn.

**`skills/lopi-cli/SKILL.md`** — this is the part that costs zero new Rust. Frontmatter description needs to trigger correctly ("when the user asks to run a coding task, check on running agents, or review lopi output"), body documents:
- `lopi run --goal "..." --repo .` and what a good goal string looks like
- `lopi watch` / `lopi tail` / `lopi dock` output shapes, so Claude can parse them without guessing
- How to interpret `AgentState` transitions (`Planning → Implementing → Testing → Scoring → OpeningPr → RollingBack`) from `LOPI_VS_OPENCLAW.md:8`, so Claude narrates status sensibly instead of treating it as opaque text

### 1.2 Engineering roadmap

**Phase 0 — kill-tests (do these before writing any packaging code):**
- Does `claude -p` (or a plugin-invoked skill) reliably shell out to a locally-built `lopi` binary from `${CLAUDE_PLUGIN_ROOT}/bin/`, or does the plugin cache copy strip execute permissions? (Plugin docs note the plugin directory is copied into a cache on install — verify the binary survives that copy with its permissions intact.)
- Does a Claude Code session that itself *is* a lopi-spawned agent, calling back into `lopi mcp-serve`, deadlock or recurse safely? (The nesting question flagged last time — lopi driving Claude Code driving lopi.)
- Does `claude plugin validate --strict` pass on a minimal skeleton before any real content is written, so the manifest schema is right from day one?

**Phase 1 — wire the MCP server (the actual unblocking work):**
- New `mcp_serve_command` in `src/cli.rs`, following the same pattern as `diag_commands.rs` / `task_commands.rs`.
- Implement `ToolHandler` for the curated tool set, calling into the same state the axum handlers use (likely needs a thin shared-state extraction so both `lopi_app::serve` and the new MCP path read from one source, not two).
- Reuse `crates/lopi-mcp/src/server.rs::serve()` as-is over stdio — it's already generic over `AsyncBufReadExt`/`AsyncWriteExt`, so this is wiring, not rewriting.

**Phase 2 — skill content:**
- Write `skills/lopi-cli/SKILL.md`.
- Defer a second skill for the `;` composer grammar until that sprint ships (flagged as non-goal above).

**Phase 3 — package + self-host:**
- Add `.claude-plugin/plugin.json` and `.claude-plugin/marketplace.json` (self-hosted catalog, one entry, pointing at `./`).
- Local test: `claude --plugin-dir ./lopi-plugin`, then `/reload-plugins`.
- `claude plugin validate --strict` clean.

**Phase 4 — submit to community marketplace:**
- `anthropics/claude-plugins-community` accepts submissions through Anthropic's automated validation + safety screening. Self-serve, no org account needed, entries get pinned to a commit SHA.
- This is additive to self-hosting, not a replacement — `/plugin marketplace add konjoai/lopi` works the moment the repo has the manifest, regardless of community-marketplace status.

### 1.3 GTM

**Positioning material already exists in the repo** — `LOPI_VS_OPENCLAW.md` is a ready-made comparison table (multi-agent pool, git-isolated worktrees per attempt, memory-augmented retry, dual TUI/web dashboard, phone control via Telegram/WhatsApp, webhook-triggered CI-failure tasks). That's the plugin's tagline and description copy, not new writing.

**Channels, roughly in order of effort:**
1. **Self-host + `/plugin marketplace add konjoai/lopi`** — day one, zero gatekeeping. This is the actual launch; everything after is amplification.
2. **Community marketplace submission** — additive discoverability once Phase 4 lands.
3. **Unofficial aggregators** — `awesomeclaude.ai`, `claudepluginhub.com`, `claudemarketplaces.com`, `claudedirectory.org` index community plugins/skills without any submission friction. Free distribution surface, worth a pass once the repo is stable.
4. **Show HN / r/ClaudeAI** — you already have `docs/screenshots/` and `docs/videos/` in the repo; a demo GIF of `lopi watch` or the web dashboard is the natural asset, no new content production needed.
5. **X/Twitter thread** — same asset reuse.

**Open scope question, not a decision made for you:** MASTER_PLAN's existing rule is "squish ships before hub repo is created." This plugin work is a new thread outside that sequencing. Worth deciding explicitly whether it waits behind squish polish and the permission-modes sprint, or runs in parallel since it's a packaging task on top of *existing* code rather than new feature work competing for the same engineering attention.

### 1.4 Timeline (relative, not calendar — depends on where it slots against current sprints)

| Week | Work |
|---|---|
| 1 | Phase 0 kill-tests; `mcp_serve_command` skeleton |
| 1–2 | Phase 1: wire `ToolHandler`, curated tool set, manual `tools/call` testing over stdio |
| 2 | Phase 2: `SKILL.md` content |
| 2–3 | Phase 3: manifest, self-host, local validation |
| 3 | Phase 4: community marketplace submission; announce |

Realistic total: **2–3 weeks of focused work**, most of it Phase 1. Everything after Phase 1 is low-risk packaging.

---

## TRACK B — MCPB Desktop Extension

### 2.1 Spec

**Same MCP server as Track A, different wrapper.** No new tool logic — `lopi mcp-serve` from Phase 1 above is reused directly, this time invoked as a bundled binary instead of a plugin-relative path.

**Bundle layout:**

```
lopi.mcpb (zip)
├── manifest.json
├── server/
│   ├── lopi-macos-arm64
│   ├── lopi-macos-x64        (optional — see build note)
│   └── lopi-windows-x64.exe
└── icon.png
```

**`manifest.json`** (current spec version `0.4`), `server.type: "binary"` — MCPB supports compiled binaries natively, no Node/Python wrapper needed:

```json
{
  "manifest_version": "0.4",
  "name": "lopi",
  "display_name": "lopi",
  "version": "0.1.0",
  "description": "Multi-agent Claude Code orchestrator for Claude Desktop.",
  "author": { "name": "KonjoAI", "email": "..." },
  "server": {
    "type": "binary",
    "entry_point": "server/${platform}/lopi",
    "mcp_config": {
      "command": "${__dirname}/server/${platform}/lopi",
      "args": ["mcp-serve"]
    }
  },
  "user_config": {
    "max_agents": { "type": "number", "title": "Max concurrent agents", "default": 4 },
    "default_repo": { "type": "string", "title": "Default repo path" },
    "claude_cli_path": { "type": "string", "title": "Claude CLI path", "default": "claude" }
  },
  "privacy_policies": ["https://github.com/konjoai/lopi/blob/main/PRIVACY.md"],
  "compatibility": { "platforms": ["darwin", "win32"] }
}
```

`user_config` fields map directly to real fields already in `lopi.toml.example` (`max_agents`, `cli_path`) — this isn't invented config surface, it's exposing what the tool already accepts.

**Privacy policy is not optional.** Sunpeak's build guidance is specific: add a "Privacy Policy" section to `README.md` *and* a `privacy_policies` array in the manifest, on `manifest_version` 0.2+. lopi's honest answer is straightforward to write since it's a local tool — no telemetry, the only outbound calls are the ones the user's own config makes (Claude CLI, GitHub API for PRs, Telegram/WhatsApp if configured) — but it needs to be a real document, not a placeholder, since a missing privacy policy is called out repeatedly across sources as an automatic rejection reason for the *directory-listed* version. (Sideloading doesn't require it, but ship it anyway — it's needed either way, might as well be correct from the start.)

### 2.2 Build difficulty — researched

**Packaging itself: low effort.** The `mcpb` CLI (`@anthropic-ai/mcpb` on npm, or the `modelcontextprotocol/mcpb` repo) handles `mcpb init` (interactive manifest scaffolding) and `mcpb pack` (zips it). Binary server type is first-class, not a workaround.

**The real cost is cross-platform builds, not packaging.** lopi is currently primary-tested on M3 (arm64 macOS). Claude Desktop only runs on macOS and Windows — no Linux desktop app — so the bundle needs at minimum a macOS arm64 build and a Windows x64 build. lopi's dependency tree (`tokio`, `sqlx`, `axum`, `teloxide`, `octocrab`, `ratatui`) is all pure-Rust or has cross-compilation stories, so this should be a GitHub Actions matrix build problem, not a rewrite — but it's untested territory for this codebase specifically, worth a kill-test before assuming it's free.

**Code signing is not an Anthropic requirement for sideloading, but it's a real practical cost.** Nothing in Anthropic's docs mandates signing to *install* a `.mcpb` via drag-and-drop. In practice: unsigned macOS binaries trigger Gatekeeper warnings, unsigned Windows `.exe` trigger SmartScreen warnings. Fixing that means an Apple Developer ID (~$99/yr) plus notarization, and a Windows code-signing certificate (varies, commonly $100–400/yr, or Azure Trusted Signing as a cheaper managed alternative). **Recommend deferring this spend** until sideload numbers show it's worth it — ship unsigned first, eat the one-time scary-dialog friction, revisit once there's a user count that justifies the cost.

**Estimate:** packaging = days once Phase 1 (shared MCP server) exists. Cross-platform build pipeline = the actual unknown, budget a week to get a clean CI matrix build working and verified on real Windows hardware, not just cross-compiled and assumed correct.

### 2.3 Submission / approval — researched

Desktop extensions have their **own submission path, separate from the Connectors Directory portal** — a dedicated "desktop extension submission form," distinct from the remote-MCP-server submission portal that lives in Claude.ai admin settings. Notably, this process does **not** appear to carry the same Team/Enterprise org gate that remote connectors do — it's not tied to the admin-settings portal. (One caveat: an older support article described this as an "interest form" / waitlist rather than a direct submission queue; a more recent doc treats it as a straightforward form. The process appears to have matured, but confirm current state at submission time rather than assuming.)

All directory submissions — MCP servers, skills, plugins, MCPBs — fall under one **Anthropic Software Directory Policy**: initial review plus ongoing compliance review, standards for "safety, security, and compatibility." No published turnaround SLA for any directory type.

**The distinction that actually matters for GTM: listed vs. usable are different things.** A `.mcpb` file downloaded from a GitHub Release installs via drag-and-drop or double-click in Claude Desktop with **zero Anthropic review** — this is the same mechanism as installing any file, gated only by the OS (Gatekeeper/SmartScreen warnings if unsigned, per above). Real precedent for this exact playbook: a `reddit-mcp-buddy` MCPB shipped and distributed entirely through GitHub Releases plus Reddit/X/Product Hunt posts while its directory submission was still pending — directory listing was pursued in parallel, not as a launch blocker.

### 2.4 GTM

Same channel list as Track A, plus the sideload-first sequencing this format specifically enables:
1. Ship `.mcpb` on a GitHub Release the moment Phase 1's shared MCP server + the build matrix both work.
2. Submit to the desktop extension directory in parallel — don't block the release on it.
3. README gets a "Quick Install" section with the direct download link and a one-line "no install, just download and double-click" pitch — this is the format's actual value proposition over the plugin path (Claude Code users are comfortable with `/plugin marketplace add`; Claude Desktop users specifically want the double-click experience).

### 2.5 Timeline

| Week | Work |
|---|---|
| (shared) | Phase 1 from Track A — the MCP server — is a hard prerequisite, don't duplicate it |
| 1 | `manifest.json`, `mcpb init`/`pack` on the arm64 build you already have |
| 1–2 | GitHub Actions cross-compile matrix (macOS arm64 confirmed, Windows x64 new territory) |
| 2 | Privacy policy doc; user_config wiring; local install test on real macOS + Windows machines |
| 2–3 | GitHub Release + README quick-install section; submit to desktop extension form |

Realistic total once Track A's Phase 1 is done: **1–2 additional weeks**, dominated by the Windows build/test cycle since that's genuinely new ground for this codebase.

---

## TRACK C — Connectors Directory (research only, build deferred)

This is the one that needs an Enterprise account to *submit*, but the *build* work is real infrastructure regardless of when you submit — worth understanding now so it's not a surprise later.

### 3.1 What it actually requires (aggregated across current docs + practitioner guides)

**Hosting:**
- A publicly reachable HTTPS server — this is the "always-on lopi server" item already on your roadmap (Fly.io), not new scope.
- **Streamable HTTP transport, not SSE.** SSE was deprecated in the March 2025 MCP spec revision; new work should target Streamable HTTP from the start. lopi's current MCP transport work (Phase 1 above) targets stdio — a remote connector needs an HTTP-framed variant of the same JSON-RPC handling, which `crates/lopi-mcp/src/server.rs`'s transport-agnostic core makes plausible, but it's still new transport code, not a config flag.

**Auth — the biggest lift:**
- OAuth 2.0 with a **real per-user consent flow**. Pure `client_credentials` machine-to-machine auth is explicitly not supported for the user-facing connector flow, even when Anthropic holds the client credentials on your behalf.
- PKCE with S256 required.
- Either Dynamic Client Registration, a Client ID Metadata Document flow, or Anthropic-held client credentials as the alternative if your auth server doesn't support DCR.
- Redirect URI to register: `https://claude.ai/api/mcp/auth_callback`. Claude Code separately needs loopback redirect support (localhost/127.0.0.1, port-agnostic) if you want Claude Code users on the connector too, not just claude.ai.
- **This directly overlaps your own in-progress research** — the memory note on "Anthropic's Managed Agents API vault system... per-session credential isolation, OAuth flows" identified as closer to your vision than BYOK-from-scratch, and the open kill-test on whether a vaulted end-user API key bills the user's own Anthropic account. That work *is* this work. Track C isn't a new research thread, it's the productionized output of research you've already scoped.

**Tool hygiene:**
- Every tool needs annotations: `readOnlyHint`, `destructiveHint`, `openWorldHint`, plus a clear title. Reviewers reportedly check this first — a tool that can cancel a task or open a PR without a `destructiveHint` is a flagged submission, not a nuanced judgment call.
- Response size discipline: ~30,000 token limit on custom-connector tool schemas, and keep individual tool *responses* short and paginated — several practitioner writeups list "returns huge unfiltered payloads" as a common rejection cause.

**Non-negotiable paperwork:**
- A live, real public privacy policy URL. Called out multiple times as an **automatic rejection** if missing or incomplete — treat this as a hard gate, not a checkbox.
- A demo/test account with realistic sample data a reviewer can use without your involvement. For lopi this likely means a seeded demo repo + a few pre-populated task records the reviewer's OAuth login lands on.

### 3.2 Submission mechanics

- Submission happens through a portal inside **Claude.ai admin settings** — this is the actual Team/Enterprise gate. By default only Owners/Primary owners have access; Enterprise can delegate via a custom role with "Directory management" (submissions only) or the broader "Libraries" permission (also covers plugins/connectors/skills).
- Listing fields: server name (100 char max), tagline (55 char max), description (2,000 char max), 1–5 categories, documentation URL, privacy policy URL, support contact.
- Reviewers reportedly run **functional tests against every declared tool** plus a policy compliance scan — not a document read-through, an actual working test of the server.
- No published SLA. Community-reported range: **two weeks to several months.**

**Important nuance that changes the sequencing calculus:** you do not need the directory listing, or an Enterprise org, for *anyone* to use a remote connector you build. Any individual on a paid plan (Pro/Max and up) can add an unlisted, directory-unlisted remote MCP server manually via Settings → Connectors → Add custom connector. The Enterprise/Team gate is specifically for the **public discoverability listing**, not for whether the thing works. That means Track C's build work (hosting + OAuth) could, in principle, ship and get real early users on a manually-added connector before an Enterprise account exists at all — the org tier only matters for the storefront, not the product.

### 3.3 Difficulty verdict

Highest of the three, and not close:

| Requirement | A (plugin) | B (MCPB) | C (connector) |
|---|---|---|---|
| New transport code | No (stdio, existing) | No (stdio, existing) | Yes (Streamable HTTP) |
| Auth | No | No | Yes (OAuth 2.0 + PKCE, real infra) |
| Hosting | No | No | Yes (always-on, public HTTPS) |
| Org account to submit | No | No (unconfirmed but appears no) | Yes (Team/Enterprise) |
| Cross-platform build | No | Yes (macOS + Windows) | No |
| Code signing pressure | No | Yes (practical, not mandated) | No |

Track C is correctly sequenced last. It's not that it's uniquely hard — it's that it needs infrastructure (hosting, OAuth) that A and B don't touch at all, on top of the org-tier gate. The honest framing: A and B are packaging problems layered on code you've already written. C is a genuine new build.

---

## 4. Combined sequencing

```
Track A ─┬─ Phase 0 kill-tests ─ Phase 1 (shared MCP server) ─┬─ Phase 2 skill ─ Phase 3 package ─ Phase 4 submit
         │                                                     │
Track B ─┴─────────────────────────────────────────────────────┴─ manifest + cross-compile ─ sign? ─ release ─ submit

Track C ─ deferred until: (a) Fly.io always-on server lands, (b) OAuth/vault kill-test resolved,
                          (c) traction from A/B justifies an Enterprise seat
```

Phase 1 is the single shared dependency both public tracks wait on. Everything downstream of it is parallelizable packaging work, not sequential engineering.

---

## 5. Master kill-test list

- [ ] Plugin cache preserves execute permissions on a bundled binary at `${CLAUDE_PLUGIN_ROOT}/bin/lopi`.
- [ ] A lopi-spawned Claude Code agent calling back into `lopi mcp-serve` doesn't deadlock or recurse unsafely.
- [ ] `claude plugin validate --strict` passes on the skeleton before content is written.
- [ ] `lopi mcp-serve`'s `ToolHandler` can read the same state `lopi_app::serve` uses without duplicating storage/auth wiring.
- [ ] Rust cross-compile to `x86_64-pc-windows-msvc` succeeds and produces a binary that actually runs `lopi mcp-serve` correctly on real Windows hardware, not just a build that compiles.
- [ ] Current desktop-extension submission path is a direct form, not still an interest-form waitlist — confirm at submission time.
- [ ] Streamable HTTP transport on top of the existing transport-agnostic `serve()` core is a wrapper, not a rewrite — spike it before committing Track C to the roadmap.
