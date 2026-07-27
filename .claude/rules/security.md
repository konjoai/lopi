---
paths:
  - "**/lopi-ui/**"
  - "**/lopi-webhook/**"
  - "**/lopi-remote/**"
  - "**/api*"
  - "**/server*"
  - "**/webhook*"
  - "**/auth*"
---
# Security Rules

- Validate all inputs at the API boundary: max goal length, max batch size, character set constraints
- Prompt injection is a real attack surface — system prompt content must never be controllable by request payload
- Never log raw user goal content at INFO level or above in production — log a hash or truncated prefix
- Rate-limit all endpoints by default
- Set and enforce per-request timeouts on every agent run
- HMAC-verify all GitHub webhook signatures (HMAC-SHA256 + constant-time comparison — already in v0.3.0, maintain it)
- WhatsApp webhook: validate the Twilio HMAC-SHA1 signature before executing any command (the Telegram transport this rule used to also cover was removed in Sprint S10, Phase 4 — see `LEDGER.md`)
- Never store API keys or tokens in the codebase — use environment variables
- Repo-supplied shell commands (`.lopi/loop.toml` `gate`/`until`/`test_command`, eval-tier-1 `Acceptance` checks) are untrusted by default — never execute one without routing through `lopi_core::resolve_guard_command`/an equivalent source-trust check (Sprint S10, Phase 0)
- Spawned `claude` CLI subprocesses get an explicit environment allowlist, never the full inherited environment (Sprint S10, Phase 1) — route any new spawn site through `apply_env_allowlist`
- MCP servers are allowlisted by name+command, deny-by-default (Sprint S10, Phase 5) — never spawn one from `.lopi/loop.toml`'s `[[mcp.servers]]` without checking it first
