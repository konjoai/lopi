---
decays: state
verified-against: 3a8a2ff
verified-date: 2026-07-24
---

# Trifecta paths — untrusted input → powerful tools → external comms

Verified against: `3a8a2ff` (`main`, v0.24.0) · 2026-07-24

Konjo Forward **F10**: lopi has the lethal trifecta by construction — untrusted content in
(webhooks), powerful tools (code execution, git, PR creation), external comms out (Telegram,
WhatsApp). Prompt injection is unsolved at the model layer, so containment has to be structural —
the runner enforces it, not a prompt. This document is the pre-flight kill-test inventory for
Sprint S2 and the post-flight re-verification after implementing it. `decays: state` — re-run this
kill-test (don't trust these citations) before building on it in a later sprint.

## 0. Bind-address check (kill-test §3)

**Confirmed live exposure, found during pre-flight — not in the sprint brief's own gap table.**

`src/cli.rs:88-91` (`Sail`) and `src/cli.rs:222-226` (`ServeWebhooks`) both default `--host` to
`127.0.0.1`. That default is correct and unchanged by this sprint.

But `fly.toml:20-21` overrides it:

```
app = "lopi serve-app --port 3002 --host 0.0.0.0"
web = "lopi sail --port 3000 --host 0.0.0.0"
```

`sail_commands::run` (`src/sail_commands.rs:103`) reads the auth token from `cfg.web.auth_token`
— the loaded `lopi.toml` — never from an environment variable. `fly.toml`'s own comment
(`fly.toml:76`) tells the operator to `fly secrets set LOPI_WEB_AUTH_TOKEN=...`, but **no code in
this repository reads `LOPI_WEB_AUTH_TOKEN`** (confirmed: `grep -rn LOPI_WEB_AUTH_TOKEN` across
`src/` and `crates/` matches only that one comment). The Dockerfile does not `COPY` a `lopi.toml`
into the image, and `fly.toml`'s process commands pass no `--config` flag, so on Fly, `cfg` is
`None` and `auth_token` is `None`.

**Net effect: the documented, currently-deployable path binds `lopi sail` to `0.0.0.0` with auth
silently disabled**, protected by nothing but obscurity of the URL. This is exactly the
"jumps to the front of this sprint" case the brief asked to check for. Phase 1's refuse-to-start
check (disabled auth + non-loopback bind → hard refusal) closes this at the code level; this
sprint additionally fixes `fly.toml`'s own secrets comment to actually be wireable (see Phase 1
below) — `LOPI_WEB_AUTH_TOKEN` needs a real read path or the Fly deployment stays broken even
after the refuse-to-start check ships (it would just fail to boot instead of booting unsafely,
which is the correct one-way trade but still needs a fix to run at all).

## 1. Untrusted-input entry points

| # | Entry point | What it creates | Reaches `lopi-remote`? | Gate before this sprint |
|---|---|---|---|---|
| A | `crates/lopi-webhook/src/github.rs:155-163` `queue_ci_fix` — any CI-failure event on a watched repo | `Task` (`TaskSource::Webhook`), goal = "Investigate and fix CI failure on {repo}" | Yes — completion fires `notify_loop` | HMAC on the webhook itself (Phase 3, see below); **none** on task execution |
| B | `crates/lopi-webhook/src/github.rs:167-198` `handle_pr_review` — a PR review with `changes_requested`, review **body text attacker-controlled** | `Task`, review body appended verbatim to `t.constraints` | Yes | Same as A |
| C | `crates/lopi-webhook/src/issue.rs:157-181` — an opened/labeled GitHub issue, Haiku-triaged then auto-queued if `Bug` @ confidence ≥ 0.7 or `lopi:fix` label. **Issue body (attacker-controlled, up to 500 chars) injected as a task constraint** | `Task`, `TaskSource::Webhook` | Yes | Same as A |
| D | `crates/lopi-remote/src/whatsapp.rs:104-111` — inbound `/task <goal>` over Twilio WhatsApp, **goal text is attacker/sender-controlled directly**, `TaskSource::Webhook { repo: "whatsapp", .. }` | `Task` | Yes | Optional Twilio signature (`signing_secret`); **but see §4 — this module is not wired to any CLI command and is unreachable in the built binary today** |
| E | `crates/lopi-remote/src/telegram/handlers.rs:181-211` — `/task`, `/retry` etc. from an authenticated Telegram chat | `Task`, `TaskSource::Telegram` | Yes | `allowed_chat_ids` inbound authz (`telegram/mod.rs:114`, checked in `message_handler`/`text_message_handler`) — this is an authenticated operator using a different transport, not the "anyone who can file an issue" threat model A–D describe |

All five converge on the same `TaskQueue` → `AgentPool` → `AgentRunner` pipeline
(`crates/lopi-orchestrator/src/pool/`), which has full tool access (code execution, git, PR
open) and, on completion, publishes `AgentEvent`s that `crates/lopi-remote/src/telegram/notify.rs`
turns into outbound Telegram messages. **Rows A–D are the trifecta**: unauthenticated or
weakly-authenticated content reaching an agent prompt, with a path to external comms on
completion. Row E is inbound-authenticated and out of this sprint's threat model (see Phase 5
below for why it's not gated the same way).

## 2. Gap-table re-derivation (sprint brief's table, checked against `3a8a2ff`)

| Gap | Brief's claim | Re-derived | Verdict |
|---|---|---|---|
| Auth is opt-in | `api_middleware.rs:17,23` | Confirmed unchanged — `auth_middleware` skips validation entirely when `s.auth_token` is `None` | **Still open — Phase 1 fixes it** |
| CORS fully permissive | `web/mod.rs:351` `CorsLayer::permissive()` | Confirmed unchanged | **Still open — Phase 2 fixes it** |
| Webhook secret optional | `lopi-webhook/src/github.rs:39,48,71` | The **library** (`lopi_webhook::serve`, `hmac_guard`) still accepts `secret: None` unverified — true as cited. But the **only production caller**, `src/webhook_commands.rs:14-30` (`enforce_webhook_secret_policy`), already refuses to start (`anyhow::bail!`) unless a secret is set or `LOPI_ALLOW_UNVERIFIED_WEBHOOK=1` is explicitly set — with its own test coverage (`webhook_commands.rs:87-127`) matching this sprint's exact verify criteria (fails fast with no secret, signed payloads pass, unsigned rejected). `git log --follow` shows this policy predates this sprint; it was not added by a prior half-finished attempt at this same brief. | **Already fixed at the only reachable entry point. Phase 3 dropped — see §3.** |
| No egress allowlist | No allowlist in `lopi-remote/src/` | Confirmed. `notify_loop` (`telegram/notify.rs`) sends to a single statically-configured `chat_id` with no allowlist check; `route_report_ready` likewise. Destination isn't attacker-influenced today (it's a config value, not derived from task content), but there is no structural allowlist and no "empty = deny" default anywhere in the send path. | **Still open — Phase 4 fixes it** |
| No trifecta-path human gate | `EarnedTrust`/`AutonomyLevel` gates autonomy generally, nothing gates untrusted-origin → external comms specifically | Confirmed, but **provenance already exists**: `TaskSource::Webhook{repo,event}` / `TaskSource::Telegram{..}` (`crates/lopi-core/src/task.rs`) and `is_untrusted_source()` (`crates/lopi-core/src/successor.rs:149-154`) already classify both as untrusted — currently used only to gate *successor chain* extension (Sprint Successor-1), not the parent task itself. The approval surface Phase 5 is told to reuse — `require_plan_approval` / `plan_gate.rs` / `AwaitingPlanApproval` / `/api/tasks/:id/plan/approve` — is fully wired end-to-end and reachable by queue-pushed tasks regardless of origin. None of rows A–D above set `require_plan_approval = true`. | **Still open — Phase 5 fixes it** |

## 3. Phase 3 correction — webhook secret enforcement is already shipped

Per the kill-test instruction ("if any [gap] is already fixed, drop that phase and record the
correction"): `lopi serve-webhooks` already refuses to boot without `LOPI_WEBHOOK_SECRET` unless
the explicit `LOPI_ALLOW_UNVERIFIED_WEBHOOK=1` escape hatch is set. This sprint makes **no code
change** for Phase 3. The residual, deliberately not closed this sprint: the *library* function
`lopi_webhook::serve()` still accepts `secret: None` if called directly, bypassing the CLI
wrapper's policy. It has exactly one caller in this codebase (`webhook_commands::run`), which
already enforces the policy before calling it — pushing the check into the library itself would
require reworking `github_tests.rs`'s `no_secret_ci_failure_queues_task` test (which deliberately
exercises the library's own unverified-request handling) for no live-exposure benefit, since
nothing else calls the unguarded path. Left as a documented, intentional layering — flagged in
`NEXT_SESSION_PROMPT.md` in case a second caller is ever added without going through
`webhook_commands`'s policy function.

## 4. Additional findings outside the sprint's five phases

Found during the kill-test; recorded here rather than silently expanded into new phases, per the
brief's own "no policy engine, most teams overshoot by one tier" caution.

- **`crates/lopi-remote/src/telegram/callbacks.rs:10-28`** — `callback_query_handler` (inline
  keyboard button presses: cancel/bump/annotate) did **not** check `allowed_chat_ids`, unlike
  `message_handler` (`handlers.rs:22`) and `text_message_handler` (`handlers.rs:112`). In practice
  a keyboard is only ever sent to a chat that already passed the inbound check (every
  keyboard-sending call site is downstream of `message_handler`'s gate), so this wasn't reachable
  by an unauthorized chat — but it directly violates the standing rule in
  `.claude/rules/security.md` ("Telegram bot: validate `chat_id` against config allowlist before
  executing any command"), and it's cheap and adjacent to Phase 4's inbound/outbound authz work on
  the same file tree. **Fixed as part of Phase 4** rather than deferred — see `CHANGELOG.md`.
- **`crates/lopi-remote/src/whatsapp.rs`** — `whatsapp::serve` is a real inbound webhook handler
  (Twilio → task queue, row D above) but is **not called anywhere outside its own crate's
  tests** — confirmed via `grep -rn "whatsapp::serve" src/ crates/`. It's dormant: reachable by
  nothing in the built `lopi` binary today. Its HMAC verification is optional-by-default in the
  same shape `github.rs`'s library layer has (§3), with no CLI wrapper enforcing a policy, because
  there is no CLI wrapper at all. Not a live exposure; flagged so whoever wires it up next doesn't
  inherit the same "fail-open on unset secret" shape without noticing. Its task-creation path did
  get the Phase 5 `require_plan_approval` gate (cheap, and `is_untrusted_source` already classifies
  its `TaskSource::Webhook` as untrusted), so at least that one containment travels with it whenever
  it's eventually wired up — the HMAC gap does not.
- **`crates/lopi-core/src/config.rs:117-119`** (`WebConfig.host`) is dead configuration — parsed
  from `lopi.toml` but never read anywhere (`grep -rn "\.web\.host"` matches nothing); `Sail`'s
  actual bind host only ever comes from the CLI `--host` flag. Not a security issue, just
  pre-existing drift noted in passing.

## 5. Post-implementation summary

All five phases resolved: four land code (Phase 3 confirmed already fixed, no change). Full
detail in `CHANGELOG.md`'s `Sprint S2` entry and `LEDGER.md`'s `Sprint S2` entry (the *why* behind
each breaking default and the two scope decisions — Telegram excluded from Phase 5's forced gate,
WhatsApp's dormant path still getting it).

- **Phase 1 (auth fail-closed):** done. `crates/lopi-ui/src/web/auth_policy.rs` (new) —
  `validate_auth_policy`, called from `sail_commands::run` before any side effect. Also closed the
  §0 Fly.io exposure: `LOPI_WEB_AUTH_TOKEN` is now actually read (`sail_commands.rs`), not just
  documented in `fly.toml`'s comments.
- **Phase 2 (CORS allowlist):** done. `crates/lopi-ui/src/web/cors_policy.rs` (new) —
  `resolve_cors_layer`, default-deny with a dev-origin fallback. Live-verified against a real
  `lopi sail` + `npm run dev` (not just router-level unit tests): the SPA round-trips through the
  vite proxy, and a direct `Origin: https://evil.example.com` request gets no
  `Access-Control-Allow-Origin` header.
- **Phase 3 (webhook secret):** no code change — already fixed at the only reachable entry point
  (§3).
- **Phase 4 (egress allowlist):** done. `crates/lopi-remote/src/egress.rs` (new) —
  `is_allowed_destination` / `check_egress`, deny-by-default, wired into `notify_loop`. Bundled the
  §4 `callback_query_handler` inbound-authz fix in the same phase (same file tree, same
  `.claude/rules/security.md` rule).
- **Phase 5 (trifecta human gate):** done. `crates/lopi-webhook/src/github.rs::gate_untrusted_source`
  (shared by `queue_ci_fix`, `handle_pr_review`, and `issue.rs`'s auto-queue via
  `crate::github::gate_untrusted_source`), plus the same one-line gate inlined in
  `crates/lopi-remote/src/whatsapp.rs`. Reuses `is_untrusted_source` (pre-existing,
  Sprint Successor-1) and `require_plan_approval`/`plan_gate.rs` (pre-existing, Phase 11) —
  no new approval mechanism. Deliberately does not extend to `TaskSource::Telegram` — see
  `LEDGER.md`.

Verified against `3a8a2ff` for the pre-flight inventory (§0–§4); the phases above landed on top of
that baseline in this same sprint. Re-run this kill-test before trusting any of it in a later
sprint — `decays: state`.
