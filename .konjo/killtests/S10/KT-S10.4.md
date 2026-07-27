# KT-S10.4 — is every untrusted-input path inventoried and gated-or-justified?

**Sprint:** S10, Phase 6 · **Verdict:** inventory complete; three paths named as
ungated by design, not by omission.

**File under test:** `docs/security/TRIFECTA_PATHS.md` §6 (new standing section).

## Method

Enumerated every path by which external text reaches an agent prompt — webhook bodies,
issue titles/bodies, PR review comments, CI logs, MCP tool responses, repository file
content, and `.lopi/loop.toml` — by tracing actual code (`crates/lopi-webhook/src/`,
`crates/lopi-agent/src/eval/`, `crates/lopi-mcp/`, `crates/lopi-core/src/loop_config.rs`),
not by re-listing the brief's own category names without checking each one against the
code. Recorded whether `gate_untrusted_source` (or an equivalent structural gate)
applies, in `docs/security/TRIFECTA_PATHS.md` §6's table (11 rows, A–K).

## Verdict

**8 of 11 rows are gated** (A–D, H, I, K — webhook-sourced tasks via
`gate_untrusted_source`'s `require_plan_approval`; `.lopi/loop.toml` `gate`/`until`/
`test_command` via Phase 0's `resolve_guard_command`; `[[mcp.servers]]` via Phase 5's
allowlist). **Row E (Telegram) is moot** — transport removed (Phase 4). **3 rows are
explicitly, individually named as not gated, with the reason stated rather than
implied:**

- **Row F — CI logs the agent fetches during its own run.** `require_plan_approval`
  fires once, before attempt 0's planning starts. Content the agent voluntarily pulls
  in later (reading a CI log, following a link) via its own tool calls is not
  re-checked against any gate. Mitigated, not closed, by Phase 3's permission-mode
  coupling (narrows what a poisoned log can talk an untrusted-sourced task's agent
  into doing) and pre-existing `DiffChecker` path restrictions.
- **Row G — repository file content.** Inherent to a code-fixing agent's job; not
  gated as a class, and not gatable without breaking the tool. Two carve-outs where
  file content crosses from "text the model reads" into "code lopi's own runtime
  executes" *are* gated (Phase 0's shell commands, Phase 5's MCP spawn allowlist) —
  the line drawn is execution vs. reading, not content vs. no content.
- **Row J — MCP tool response content, from an allowlisted server.** Phase 5 pins
  *which* server binaries may run; it does not, and structurally cannot without solving
  prompt injection at the model layer, sanitize what an allowlisted server's tool call
  *returns*. The postmark-mcp shape (fifteen clean releases, then one malicious update)
  defeats binary-identity pinning alone if the compromise happens server-side rather
  than in the spawned binary — named as the natural next question, not answered here.

**This is not a claim of full coverage.** Rows F, G, and J are the ones with no
realistic full gate short of solving prompt injection at the model layer — recorded as
accepted, structurally-mitigated risk, exactly as the sprint's own anti-goal demands
("do not treat the audit's stated gaps as clean"). The inventory's value is in making
these three explicit and re-derivable, not in claiming they're closed.

Re-run this inventory (`docs/security/TRIFECTA_PATHS.md` §6) whenever a new
external-input path is added — `decays: state`.
