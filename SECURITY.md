# Security Policy

## Supported versions

lopi is pre-1.0 and moves fast. Security fixes land on `main` and are
supported only in the latest published release (`v0.31.0` and forward).
There is no long-term-support branch and no backport policy for older
tags — upgrade to the latest release to pick up a fix.

| Version | Supported |
|---|---|
| latest `main` / latest release | ✅ |
| anything older | ❌ |

## Reporting a vulnerability

Please **do not** open a public GitHub issue for a security finding.

Report privately via **GitHub Security Advisories**:
<https://github.com/konjoai/lopi/security/advisories/new>

If you cannot use GitHub Security Advisories, open a regular issue asking
a maintainer to reach out over a private channel — do not include
exploit details in that issue itself.

Include, where you can:

- The affected version/commit.
- A minimal reproduction — a `.lopi/loop.toml` snippet, request, or
  command sequence.
- What you'd expect lopi to do instead, and why the actual behavior is a
  security issue rather than a correctness bug (lopi runs an autonomous
  coding agent with real tool access — a finding doesn't need to be a
  classic memory-safety bug to matter here; a containment gap around
  untrusted input, permission posture, or supply chain counts too).

## Response expectations

- **Acknowledgment:** within 3 business days.
- **Initial triage** (confirmed / not a security issue / need more info):
  within 7 business days.
- **Fix or mitigation timeline:** communicated once triage is done,
  scaled to severity — a structural containment gap (untrusted input
  reaching shell execution, an auth bypass, a credential leak) is treated
  as the top-priority sprint per this repo's own Konjo Quality Framework
  ordering (see `KONJO_QUALITY_FRAMEWORK.md`), ahead of feature work.
- We'll credit reporters in the fix's changelog entry unless you ask us
  not to.

## Scope

In scope: the `lopi` Rust workspace (`crates/`, `src/`), the web
dashboard (`web/`), the macOS app (`macos/`), the Claude Desktop
extension (`mcpb/`), and the Claude Code plugin (`plugin/`) in this
repository.

Out of scope: vulnerabilities in third-party dependencies with no
lopi-specific exploitation path (report those upstream — `cargo audit`/
`cargo deny check` run as blocking CI gates on every PR, and known
advisories are triaged in `.cargo/audit.toml` / `.konjo/deny.toml`, each
with a written reason, not a blanket suppression).

## What "secure" means for lopi specifically

lopi runs an autonomous coding agent with git, shell, and (by default)
broad tool access against a real repository. Prompt injection is
unsolved at the model layer (see `docs/security/TRIFECTA_PATHS.md`'s
Non-goals) — lopi's own security posture is about **structural
containment**, not preventing a model from being told something
misleading:

- Untrusted-sourced input (a GitHub webhook, an issue body, a PR review)
  is held for human plan-approval before the agent acts on it.
- Repo-supplied shell commands (`.lopi/loop.toml` `gate`/`until`/
  `test_command`, `Task.acceptance`'s shell/suite checks) refuse to run
  unless they came from the operator's own config, not a branch under
  evaluation.
- The spawned `claude` CLI subprocess gets an explicit environment
  allowlist, never lopi's own full process environment.
- An untrusted-sourced task's permission posture is coupled to that
  distrust — it never runs under an unattended-approval mode regardless
  of what it requests.
- MCP servers are spawned only from an operator-approved allowlist,
  deny-by-default.

Full inventory and the reasoning behind each: `docs/security/TRIFECTA_PATHS.md`,
`docs/security/EGRESS_SURFACE.md`, and `LEDGER.md`.
