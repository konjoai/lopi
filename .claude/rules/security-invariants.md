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
# Security Invariants

Class rules: what to check, timeless. See `security-sinks.md` for the call-site
inventory (where each is enforced today, and its provenance).

- Validate all inputs at the API boundary: max goal length, max batch size, character set constraints
- Prompt injection is a real attack surface — system prompt content must never be controllable by request payload
- Never log raw user goal content at INFO level or above in production — log a hash or truncated prefix
- Rate-limit all endpoints by default
- Set and enforce per-request timeouts on every agent run
- HMAC-verify webhook signatures with a constant-time comparison before trusting the payload
- Never execute a command a webhook triggers before its signature check passes
- Never store API keys or tokens in the codebase — use environment variables
- Repo-supplied shell commands are untrusted by default — never execute one without routing through a source-trust check
- Spawned subprocesses get an explicit environment allowlist, never the full inherited environment
- Third-party service integrations (MCP servers, webhooks) are allowlisted by name and command, deny-by-default
