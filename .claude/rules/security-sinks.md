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
# Security Sinks

Call-site inventory for `security-invariants.md`'s class rules: where each is enforced
today, and the sprint that landed it. A citation here is provenance, not a class rule —
see `security-invariants.md` for what every change in these paths must satisfy.

- GitHub webhook signatures: HMAC-SHA256 + constant-time comparison, `lopi-webhook` (already in v0.3.0, maintain it)
- WhatsApp webhook: Twilio HMAC-SHA1 signature check, `crates/lopi-remote/src/whatsapp.rs`'s `check_signature` (the Telegram transport this rule used to also cover was removed in Sprint S10, Phase 4 — see `LEDGER.md`)
- Repo-supplied shell commands (`.lopi/loop.toml` `gate`/`until`/`test_command`, eval-tier-1 `Acceptance` checks): routed through `lopi_core::resolve_guard_command` (Sprint S10, Phase 0)
- Spawned `claude` CLI subprocesses: environment allowlist via `apply_env_allowlist` (Sprint S10, Phase 1) — route any new spawn site through it
- MCP servers: allowlisted by name+command, deny-by-default, checked before spawn from `.lopi/loop.toml`'s `[[mcp.servers]]` (Sprint S10, Phase 5)
