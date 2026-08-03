---
decays: state
verified-against: 824ba65
verified-date: 2026-08-03
---

# Trifecta paths — untrusted input → powerful tools → external comms

Verified against: `824ba65` · 2026-08-03 (re-verified; G0 flagged this doc stale again — the
review-pipeline Sprint P2 branch's own single commit, a `.gitignore`/`LEDGER.md`-only change
recording a mutation-testing baseline launch, pushed the commit count crossing `a2f6f78` past the
20-commit cap via the PR's merge ref, the same "commit volume, not content drift" trigger noted
twice below. Diffed every commit `a2f6f78..824ba65` against every file this doc cites (full list:
`src/cli.rs`, `src/sail_commands.rs`, `crates/lopi-remote/src/whatsapp.rs`,
`crates/lopi-webhook/src/github.rs`, `crates/lopi-webhook/src/issue.rs`,
`crates/lopi-core/src/task.rs`, `crates/lopi-core/src/successor.rs`,
`crates/lopi-core/src/config.rs`, `crates/lopi-orchestrator/src/pool/run_loop.rs`,
`crates/lopi-git/src/diff.rs`, `crates/lopi-git/src/manager.rs`, `crates/lopi-git/src/worktree.rs`,
`crates/lopi-agent/src/prompt.rs`, `crates/lopi-agent/src/runner/stability_runner.rs`,
`crates/lopi-agent/src/runner/test_phase.rs`, `crates/lopi-agent/src/runner/finalize.rs`,
`crates/lopi-agent/src/claude_spawn.rs`, `crates/lopi-core/src/permission_mode.rs`,
`crates/lopi-ui/src/web/*.rs`, `web/src/lib/api.ts`, `fly.toml`, macOS/`project.yml`). Only one
changed: `crates/lopi-core/src/task.rs` gained an unrelated `tool_profile` field (Sprint P1,
review-pipeline Phase 1) — two insertions shifted `Task::new`'s body, moving §8 row 5's
`source: TaskSource::Cli` citation from `391-429` to `421-464`, corrected below. The claim itself
(`Task::new` defaults `source` to `TaskSource::Cli`, i.e. trusted) is unchanged — confirmed by
reading the current function body, not assumed from the line-shift alone. No other cited file in
the list above changed in this window; no content-level drift found anywhere. This branch's own
change (`.gitignore` + `LEDGER.md`, recording a `cargo mutants --workspace` baseline launch) touches
none of this doc's cited paths.

Superseded prior banner (`a2f6f78` · 2026-07-29 — merge of two independent re-verifications, neither of
which touched the other's cited files — no new drift from combining them. This sprint's own commit
volume crossed the 20-commit cap a second time, not from new content drift. One more citation
shifted since the `ef41e7f` pass below: the `gate_polarity` triage's fix to
`whatsapp.rs`'s `check_signature` (naming the no-secret branch's `Ok(())` via
`verification_disabled_override()`) added one line above row D's `/task` handler, so
`128-142` → `129-142`, corrected above. No other citation moved. The other re-verification
(`b93e68f` · 2026-07-28, this PR's own): of this doc's cited files, only five had changed since the
prior `e2f9362` checkpoint — `src/cli.rs` gained new `Cost`/`Rates` commands ahead of `Sail`/
`ServeWebhooks` (citation unaffected, see §0), `src/sail_commands.rs` only wired the new economics
pool into `AgentPool`, `crates/lopi-core/src/config.rs` only added an `economics` field,
`crates/lopi-orchestrator/src/pool/run_loop.rs` only added reservation-cleanup calls after the §8
row 1 repo-resolution block, and `crates/lopi-remote/src/whatsapp.rs`'s only change was the
`/cost` command (no new row; row D's citation is tracked by the `gate_polarity` shift above, not
this change) — none of it drifted this doc's content. This PR's own changes are
`web/src/lib/components/stacks/*` (Loop Stacks composer/xN-grammar/logs-panel work) and
`web/src/routes/+layout.svelte` — UI-only, touching none of this doc's cited paths. Prior banner
(`ef41e7f` ·
2026-07-29 (re-verified; Sprint S13R's Phases C-E touched four
files this doc cites — `crates/lopi-core/src/config.rs`, `crates/lopi-core/src/task.rs`,
`crates/lopi-git/src/diff.rs`, `crates/lopi-git/src/manager.rs`, and
`crates/lopi-remote/src/whatsapp.rs` — while converting `anyhow` call sites to typed errors
(Phase E) and resolving a `gate_polarity` finding (Phase A). Diffed each against this doc's
citations: no content-level drift in any of them (every changed function's *behavior* is
unchanged — `check_signature`'s no-secret branch still returns the same `Ok(())`, just via a
newly named `verification_disabled_override()`; `check_diff_scope`/`DiffChecker::validate` still
enforce the same allow/forbid logic, just with a typed `DiffScopeError` instead of `anyhow`), but
four line-citations drifted and are corrected here: `whatsapp.rs`'s row-D `/task` handler
(§1, §6 row K) `120-134` → `128-142` (an 8-line insertion above it); `config.rs`'s `WebConfig.host`
(§4) `143` → `163` (a 20-line insertion above it); `diff.rs`'s `DiffChecker` citation (§8 row 2)
`12-63` → `29-80` (a 17-line insertion above it); `manager.rs`'s `check_diff_scope` citation
(§8 row 2) `100-127` → `100-128` (the function itself grew by one line). `task.rs`'s
`from_toml_str` change (a like-for-like 3-line replacement, zero net line delta) touches no cited
range. No other cited file changed since `b93e68f`. Superseded prior banner (`b93e68f` ·
2026-07-28) covered `e2f9362..b93e68f` and is left as historical record below — see that entry for
the Sprint E economics-layer citation fixes it made; nothing here revisits it. §0's other
citations and §2's citations describe the pre-Sprint-S2 baseline at `3a8a2ff` by design and are
left as historical record — see §5 for what actually shipped.

Prior verification banner (`b93e68f` · 2026-07-28): Verified against: `b93e68f` · 2026-07-28
(re-verified; G0 flagged this doc stale — 20+ commits past `e2f9362` — after an unrelated Sprint
S13 Phase 0 PR landed a commit on top of it. Diffed every commit `e2f9362..b93e68f` against every
file this doc cites: `crates/lopi-core/src/config.rs`, `crates/lopi-remote/src/whatsapp.rs`,
`crates/lopi-orchestrator/src/pool/`, `crates/lopi-ui/src/web/mod.rs`, `src/cli.rs`, and
`src/sail_commands.rs` all changed (Sprint E's economics layer + `lopi cost`/`lopi rates` CLI
commands); every other cited file (`github.rs`, `issue.rs`, `task.rs`, `successor.rs`,
`permission_mode.rs`, `diff.rs`, `manager.rs`, `worktree.rs`, `prompt.rs`, `stability_runner.rs`,
`test_phase.rs`, `claude_spawn.rs`, `api_middleware.rs`, `auth_policy.rs`, `cors_policy.rs`,
`ws_ticket.rs`, `src/mcp_commands/mod.rs`) is untouched since `e2f9362` — no drift possible there.
Of the changed files: two real line-citation drifts found and fixed — `src/cli.rs`'s
`Sail`/`ServeWebhooks` `--host` citations (§0) and `config.rs`'s `WebConfig.host` citation (§4) had
drifted (the `Sail`/`ServeWebhooks` citations were already stale even at the `e2f9362` checkpoint,
predating this window — corrected now regardless). `whatsapp.rs`'s row-D `/task` handler citation
(§1, §6 row K) is unchanged (Sprint E's `/cost` addition landed after it, not before).
`crates/lopi-orchestrator/src/pool/`'s new `economics_admit.rs`/`runaway_monitor.rs` gate budget
admission, not tool access or trust classification — §1's "full tool access" claim is unaffected.
`web/mod.rs`'s one-line change adds `/api/economics` to the `protected` router — live confirmation
§7's structural fix (every route lives in `protected` or the public fallback, no third place) is
still holding. No content-level drift found in any changed file; only the two line-number
citations above needed correcting.)

Konjo Forward **F10**: lopi has the lethal trifecta by construction — untrusted content in
(webhooks), powerful tools (code execution, git, PR creation), external comms out (Telegram,
WhatsApp). Prompt injection is unsolved at the model layer, so containment has to be structural —
the runner enforces it, not a prompt. This document is the pre-flight kill-test inventory for
Sprint S2 and the post-flight re-verification after implementing it. `decays: state` — re-run this
kill-test (don't trust these citations) before building on it in a later sprint.

## 0. Bind-address check (kill-test §3)

**Confirmed live exposure, found during pre-flight — not in the sprint brief's own gap table.**

`src/cli.rs:95-96` (`Sail`) and `src/cli.rs:286-287` (`ServeWebhooks`) both default `--host` to
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
| A | `crates/lopi-webhook/src/github.rs:157-167` `queue_ci_fix` — any CI-failure event on a watched repo | `Task` (`TaskSource::Webhook`), goal = "Investigate and fix CI failure on {repo}" | Yes — completion fires `notify_loop` | HMAC on the webhook itself (Phase 3, see below); **none** on task execution (pre-Phase-5; now gated by `gate_untrusted_source`, see §5) |
| B | `crates/lopi-webhook/src/github.rs:184-221` `handle_pr_review` — a PR review with `changes_requested`, review **body text attacker-controlled** | `Task`, review body appended verbatim to `t.constraints` | Yes | Same as A |
| C | `crates/lopi-webhook/src/issue.rs:159-181` — an opened/labeled GitHub issue, Haiku-triaged then auto-queued if `Bug` @ confidence ≥ 0.7 or `lopi:fix` label. **Issue body (attacker-controlled, up to 500 chars) injected as a task constraint** | `Task`, `TaskSource::Webhook` | Yes | Same as A |
| D | `crates/lopi-remote/src/whatsapp.rs:129-142` — inbound `/task <goal>` over Twilio WhatsApp, **goal text is attacker/sender-controlled directly**, `TaskSource::Webhook { repo: "whatsapp", .. }` | `Task` | Yes | Optional Twilio signature (`signing_secret`); **but see §4 — this module is not wired to any CLI command and is unreachable in the built binary today** |
| E | ~~`crates/lopi-remote/src/telegram/handlers.rs:181-211`~~ — **transport removed, Sprint S10 Phase 4.** Historical rows with `TaskSource::Telegram` still deserialize and read as `provenance: "operator"` (`TaskRow::provenance()`); `is_untrusted_source` still classifies the variant as untrusted for chain-depth purposes (Successor-1) — a different, narrower notion of "untrusted" than this row ever used, see `LEDGER.md`. Nothing constructs this variant anymore. | (historical only) | — (no longer reachable) | Moot — removed rather than gated |

Rows A–D converge on the same `TaskQueue` → `AgentPool` → `AgentRunner` pipeline
(`crates/lopi-orchestrator/src/pool/`), which has full tool access (code execution, git, PR
open) and, on completion, publishes `AgentEvent`s. **Rows A–D are the trifecta**: unauthenticated
or weakly-authenticated content reaching an agent prompt, with a path to external comms on
completion (WhatsApp today; Telegram before Phase 4's removal). Row E was inbound-authenticated
and out of Sprint S2's threat model even before Phase 4 removed the transport it described — see
that sprint's Phase 5 note below for why it was never gated the same way as A–D.

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
  `.claude/rules/security-invariants.md` ("Telegram bot: validate `chat_id` against config allowlist before
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
  Sprint E (Finding #10) added a `/cost` command to this same handler — read-only (formats a
  unit-economics report from the local ledger, no `Task` construction, no external call), so it
  adds no new row to this table and doesn't change the "dormant, no CLI wrapper" status above.
- **`crates/lopi-core/src/config.rs:163`** (`WebConfig.host`) is dead configuration — parsed
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
  `.claude/rules/security-invariants.md` rule).
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

## 6. Sprint S10 — untrusted-source inventory (standing section)

Phase 6's mandate: enumerate every path by which external text reaches an agent prompt, and
record whether `gate_untrusted_source` (or an equivalent structural gate) applies. Not a
point-in-time finding like §0–§5 — this section is meant to be extended, not re-derived from
scratch, whenever a new external-input path is added. Agentjacking (13 June 2026, Tenet
Security) is the general rule this section exists to keep honest: treat any output an agent
reads from outside lopi's own trusted config as untrusted input, and name the ones that
currently reach a prompt ungated rather than letting a clean-looking table imply full coverage.

| # | Path | Reaches the agent as | Gated? |
|---|---|---|---|
| A–D | Webhook bodies, issue titles/bodies, PR review comments (see §1) | `Task.goal`/`Task.constraints` | Yes — `gate_untrusted_source` forces `require_plan_approval` before attempt 0 plans |
| F | CI logs / `gh` output the agent fetches **during its own run** (e.g. investigating a failure `queue_ci_fix` queued) | Tool-call output folded into the CLI session's own context | **No.** `require_plan_approval` fires once, before planning starts — content the agent voluntarily pulls in in a later tool call (reading a CI log, `curl`-ing a URL if permitted, opening a linked issue) is not re-checked. Structural containment for this class is Phase 3's permission-mode coupling (`effective_permission_mode` forces `DontAsk` for untrusted-sourced tasks, narrowing what a poisoned log can talk the agent into doing) plus `DiffChecker`'s off-limits paths — not a content filter, since none exists at the model layer (see Non-goals). |
| G | Repository file content (source files, `.lopi/loop.toml`, README, CI config) the agent reads as part of normal operation, including on a branch under evaluation | Tool-call (`Read`) output folded into context | **Inherent, not gated as a class.** A code-fixing agent must read the repo it's fixing; this is the tool doing its job, not a bypassable injection point. Two carve-outs *are* gated because they cross from "text the model reads" to "code lopi's own runtime executes": `.lopi/loop.toml`'s `gate`/`until`/`test_command`/`[[mcp.servers]]` (Phase 0, Phase 5 — see rows H/I) and `Task.acceptance`'s `Shell`/`Suite` checks (Phase 0). |
| H | `.lopi/loop.toml` `gate`/`until`/`test_command` — a shell string, not model-read text | `sh -c` execution via `run_guard_command` | **Yes, Phase 0.** `resolve_guard_command` refuses a repo-supplied value unless the task's source is trusted (`!is_untrusted_source`) or the operator's own `~/.lopi/loop.toml` sets it. `Task.acceptance`'s `CheckSpec::Shell`/`Suite` gated the same way via `EvalContext.shell_commands_trusted`. |
| I | `.lopi/loop.toml` `[[mcp.servers]]` — a `command`+`args` pair | `Command::new(command).args(args).spawn()` via `McpServerSpec::connect` | **Yes, Phase 5.** `check_mcp_server` refuses to spawn unless the exact `(name, command, args)` is in the operator's `~/.lopi/mcp_allowlist.toml` — deny-by-default, no fallback to "unrestricted" on an empty/missing file. |
| J | MCP tool **response** content, from an allowlisted server | Tool-call output folded into context, same as any other tool result | **Not gated, named rather than implied.** Phase 5 pins *which* server binaries may run; it does not — and structurally cannot, without solving prompt injection at the model layer (Non-goals) — sanitize what an allowlisted server's tool call *returns*. A compromised update to an allowlisted binary (the postmark-mcp shape: fifteen clean releases, then one malicious line) still returns attacker-controlled content once spawned. Signature verification (noted, not built, in Phase 5) would catch a *changed* binary; it would not catch a legitimate server whose upstream API was itself compromised. |
| K | WhatsApp inbound (row D) | `Task.goal` | Yes, same as A–D — and still dormant/unreachable from the built binary (§4) |

Every row without a "Yes" is a deliberate, named gap — not an oversight this document is hiding.
Rows F, G, and J are the ones with no realistic full gate short of solving prompt injection at
the model layer; Phase 3's permission-mode coupling and `DiffChecker` reduce blast radius for
all three without claiming to close them.

Re-run this inventory whenever a new external-input path is added (a new webhook event type, a
new MCP capability, a new `.lopi/loop.toml` field that can hold a command or spawn a process) —
`decays: state`.

## 7. Sprint S11 Round 2 — the streaming/observability surface (standing addition)

Not a trifecta path in §1's sense (no untrusted input reaches an agent prompt through it), but
the same "reachable with nothing but the URL" exposure class §0 named for the bind address —
recorded here because it's the same document's job: everything reachable on the listening port
with less than the intended credential.

**Finding (Phase 0, BLOCKING):** `/sse`, `/ws`, `/ws/tasks`, `/metrics` were registered on the
*outer* `Router` in `crates/lopi-ui/src/web/mod.rs::build_app`, after `.merge(api)` — outside the
`route_layer` calls that apply `auth_middleware`/`rate_limit_middleware` to everything registered
*before* them on the same router instance. Live-verified against a real binary with a real
`auth_token` configured (`.konjo/killtests/S11/KT-S11.0.md`): all three streamed in full —
`/ws`'s connect-time snapshot includes the last 100 tasks, per-task cost, and status counts —
with zero `Authorization` header, while `/api/health` on the same server correctly 401'd in the
same run. On the documented Fly.io deployment (§0, still `--host 0.0.0.0`), this was reachable
from the public internet by URL alone.

**Fix:** structural, not four bolted-on checks — every route now lives in exactly one of two
places: the single `protected` router (Bearer-or-ticket auth + per-IP rate limiting, via
`route_layer`) or the outer router's one explicit public entry (the static/SPA `fallback`). A
route added to `protected` inherits both layers automatically; there is no third place to
register a route that skips them. `/ws`, `/ws/tasks`, `/sse` additionally accept a single-use,
30-second ticket (`?ticket=`, minted by authenticated `POST /api/ws-ticket`,
`crates/lopi-ui/src/web/ws_ticket.rs`) as a browser-compatible alternative to the header — a
`WebSocket`/`EventSource` upgrade can't set custom headers, so the header-only design was itself
part of why these routes were awkward to fold into the existing auth shape. `/metrics` accepts
no ticket: a Prometheus scraper sets an `Authorization` header like any other HTTP client.

**Verify:** `crates/lopi-ui/src/web/streaming_auth_tests.rs` (per-endpoint 401s, ticket mint/
consume/single-use/scope), `crates/lopi-ui/src/web/route_coverage_tests.rs` (every registered
route enumerated and asserted either 401-without-token or on the explicit public allowlist —
the gate Phase 4 asked for, with its own hand-maintained-list limitation named in its doc
comment), `.konjo/killtests/S11/KT-S11.0.md` (live pre-fix/post-fix curl evidence).

**Named, not closed:** the web dashboard's own `fetch()` calls (`web/src/lib/api.ts`) attach no
`Authorization` header at all — confirmed by grep, zero call sites. Every documented deployment
path (`docs/RUNNING.md`) runs the SPA with `--insecure-no-auth` on loopback, where this doesn't
matter (`auth_token` is `None`, nothing is checked). Against a server with a real `auth_token`
configured (the Fly.io / non-loopback case §0 and this section both describe), the SPA's
`/api/*` calls already 401 today — a pre-existing gap this sprint did not introduce and does not
fix; see `LEDGER.md`'s Sprint S11 entry for why it's named here rather than silently assumed
solved by the ticket mechanism.

Re-run this section's kill-test whenever a new route is added to `build_app` — `decays: state`.

## 8. Sprint S12, Phase 3 — task-scope confinement (reframed post-scope-lock)

Sprint S12 locked lopi to one operator, one machine (see `LEDGER.md`). That retires
cross-tenant IDOR as a question, but not authorization outright: lopi still has three
principals — the operator (bearer token), the agent (runs with lopi's privileges, reads
repo content it doesn't control), and an untrusted-source task (webhook/PR-originated,
already tagged via `is_untrusted_source`). KT-S12.3's question: **can an untrusted-source
task, or an agent acting on the operator's behalf, act on a repo/path/command the operator
never authorized for it?** This is an inventory, not a pass/fail — every row below either
names a real enforcement mechanism or is marked unenforced plainly, per the sprint's own
instruction not to let a clean table imply coverage that isn't there.

| # | Question | Enforcement | file:line | Verdict |
|---|---|---|---|---|
| 1 | Repo confinement — can a task run against a repo outside the operator's configured `repo`/`extra_repos`? | None. `task.repo_path` (if set) is used verbatim; if unset, whichever `AgentPool` dequeues the task supplies its own default. No allowlist exists to check against — `LopiConfig` has no `repo`/`extra_repos` field at all; those are CLI flags passed straight into `AgentPool::new` as *defaults*, never consulted again downstream. | `crates/lopi-orchestrator/src/pool/run_loop.rs:99-104` (resolution, no check); `src/mcp_commands/mod.rs:297-298` (`lopi_submit_task` sets `repo_path` from an untyped string, zero validation) | **Unenforced** |
| 2 | `allowed_dirs`/`forbidden_dirs` — structural or advisory? | Two mechanisms at two pipeline stages. Pre-hoc: injected into the system prompt and checked by the stability harness's plan-sample review, which only warns (`stability_runner.rs`'s own comment: "advisory — the real diff is still enforced separately"). Post-hoc: `DiffChecker`/`check_diff_scope` inspects the actual worktree diff after implementation and rolls the attempt back (`TaskStatus::RolledBack`) on violation. | Advisory: `crates/lopi-agent/src/prompt.rs:27-38`, `crates/lopi-agent/src/runner/stability_runner.rs:54-60`. Structural: `crates/lopi-git/src/diff.rs:29-80`, `crates/lopi-git/src/manager.rs:100-128`, called from `crates/lopi-agent/src/runner/test_phase.rs:60-68,277-283` | **Mixed** — prompt/stability-harness layer is advisory-only; `DiffChecker` is real enforcement, but post-hoc (blocks the diff from persisting/PR-ing, does not prevent the write itself) |
| 3 | Can an untrusted-source (webhook) task be routed to a repo the operator never associated with that source? | Webhook-originated tasks (`queue_ci_fix`, `handle_pr_review`, issue triage) never set `task.repo_path` — the attacker-controlled `repository.full_name` from the payload is stored only as `TaskSource::Webhook{repo,..}` metadata, never used to select or validate a filesystem path. The task lands on whichever repo the dequeuing pool defaults to (see row 1) — there is no per-repo webhook watch-list cross-checking the payload's claimed repo against the pool it's about to run in. | `crates/lopi-webhook/src/github.rs:157-167,184-221`, `crates/lopi-webhook/src/issue.rs:159-181` | **Unenforced** (repo targeting is provenance-blind in both directions — same root cause as row 1) |
| 4 | Worktree escape via symlink, absolute path, or `..` in a tool call | None found. `crates/lopi-git/src/worktree.rs` sanitizes only the worktree's own directory name (flattening `/`/`\` in the task id) — it does not validate paths a tool call touches once inside the checkout. Confinement is entirely by convention: the spawned `claude` CLI gets `current_dir` set to the worktree, and (absent a tighter per-task mode) `--permission-mode bypassPermissions` by default. lopi does not intercept or path-validate individual tool calls — that would require proxying the CLI's own tool execution, which is out of this phase's scope (Non-goals: no policy engine). | `crates/lopi-git/src/worktree.rs:332-338` (`sanitize`, dir-name only); `crates/lopi-agent/src/claude_spawn.rs:127` (`cmd.current_dir(...)` is the entirety of the confinement); `crates/lopi-core/src/permission_mode.rs:29-53` (`BypassPermissions` is `#[default]`) | **Unenforced** — named, not fixed this sprint; a real fix is a sandboxing/proxying project, not a targeted patch |
| 5 | `gate_untrusted_source` coverage — does every untrusted-input path in §6's table actually route through it? | Yes for every row already in §6 (A–D, K): defined once in `crates/lopi-webhook/src/github.rs:177-181`, called from `queue_ci_fix` (row A, `:164`), `handle_pr_review` (row B, `:218`), issue triage (row C, via the same function), and WhatsApp's inline equivalent (row D/K). Successor/chained tasks are separately and correctly gated via `derive_successor_task` (`crates/lopi-core/src/successor.rs:264-268` forces `require_plan_approval=true`/`successor_enabled=false` when the parent is untrusted; enforced at `crates/lopi-agent/src/runner/finalize.rs:161`). **New gap, not in §6 at all:** `lopi_submit_task` (the MCP tool) never calls `is_untrusted_source`/`gate_untrusted_source` — it builds `Task::new()` (source defaults `Cli`, i.e. trusted) directly from caller-supplied JSON, including an arbitrary `repo` and `permission_mode` (up to and including `bypassPermissions`), with no plan-approval gate. | Gap: `src/mcp_commands/mod.rs:294-341` (`submit_task`), `crates/lopi-core/src/task.rs:421-464` (`Task::new` defaults `source: TaskSource::Cli`) | **Enforced for A–D/K; unenforced for the `lopi_submit_task` MCP path** |

### Why row 5's MCP gap is named, not patched, this sprint

`lopi_submit_task` is reachable two ways that need opposite treatment, and lopi's MCP server
cannot currently tell them apart:

- **The operator, interactively, asking their own Claude Code session to submit a lopi
  task.** This is the tool's whole purpose and is exactly as trusted as typing the goal into
  `lopi run` directly — forcing `require_plan_approval` here would degrade a legitimate,
  common workflow for no security benefit.
- **A nested agent session, already running a task lopi itself queued, that got
  prompt-injected by content it read** (a malicious code comment, a poisoned issue body) and
  calls `lopi_submit_task` to spawn a *fresh* task with full trust and no plan-approval gate —
  functionally an ungated alternate path to what `derive_successor_task` already handles
  correctly for the chained-task case.

Distinguishing these needs the MCP server to know the trust level of *whichever agent session
called it*, which lopi's stdio MCP transport does not currently propagate. A blanket fix
(always require plan approval) breaks the first, legitimate case for no benefit against the
second; a correct fix needs actual session-provenance plumbing, which is a real feature, not a
one-line patch — building it without that context is exactly the kind of policy-engine
over-reach the sprint's Non-goals rule out. Recorded here as a deliberate, unfixed gap for a
follow-up sprint that scopes the plumbing properly, not silently left off this table.

### Documentation-drift finding

`crates/lopi-core/src/config.rs`'s `[lopi].bypass_permissions` doc comment implies it drives
real directory-access restriction. It does not: its only consumer is `src/repl/state.rs:67`,
where it is read purely as TUI display state. It enforces nothing. Flagged as drift to fix
independent of the confinement questions above — a doc comment overstating what a config field
does is itself a trap for the next person relying on it as a security control.

## 9. Sprint S12, Phase 4 — Swift review (macOS/iOS app)

`macos/` + `packages/LopiStacksKit/` (~19k LOC; not a static `.xcodeproj` — XcodeGen's
`project.yml` generates `Info.plist`/`.entitlements` at build time, so those are the source of
truth, not files in the tree). Scoped to what can hurt on a single-user machine per the S12
scope lock — no multi-tenant concerns apply here.

| # | Area | Verdict | Detail |
|---|---|---|---|
| 1 | Keychain usage beyond `ServerConfig` | **Clean** | `Keychain` enum (`macos/Lopi/Store/ServerConfig.swift:47-85`) is the only Keychain call site in the tree — correct generic-password wrapper (`kSecClassGenericPassword`, fixed service, delete-before-write). Everything else persisted via `UserDefaults` (host/port, accent theme, pane layout, launch-controls defaults, budget alert numbers, the local stack-template library) is genuinely non-sensitive UI/config state — no token or credential reaches `UserDefaults`. |
| 2 | URL / deep-link handling | **Clean — no surface exists** | No `CFBundleURLSchemes`, `onOpenURL`, `NSUserActivity`, or associated domains anywhere in `macos/` or `project.yml`. Both app entry points declare only plain `WindowGroup`/`MenuBarExtra`/`Settings` scenes. Nothing to trace. |
| 3 | Rendering agent output | **Clean** | No `WKWebView`/`UIWebView`/`loadHTMLString`/HTML-mode `NSAttributedString` anywhere. `MarkdownLogView.swift:56-61` renders agent text via SwiftUI's native `AttributedString(markdown:)` (text-only, no HTML/script execution path), with a plain-text fallback on parse failure. Not the stored-XSS shape S11 Phase 2 found on the web side. |
| 4 | App Transport Security exceptions | **Clean — strict default, no exceptions declared** | No `NSAppTransportSecurity`/`NSAllowsArbitraryLoads`/`NSExceptionDomains` anywhere; `project.yml` sets no ATS overrides. Default ATS should block plain `http://` to any non-loopback host outright (loopback is Apple's own built-in exemption, which is what makes the default `http://127.0.0.1:3000` config work). |
| 5 | Entitlements | **Clean / minimal** | macOS target: sandboxed, `network.client: true`, `network.server: false` (correct — the app is a client of `lopi sail`, not a listener), no file-access entitlements requested (no file-picker/import-export feature exists). iOS target declares no extra entitlements. Nothing overbroad. |
| 6 | Unencrypted disk writes | **Clean** | No production `FileManager`/`.write(to:)`/`Data(contentsOf:)` writes of sensitive content found. Task transcripts and live agent state are held purely in-memory (`@Observable` structs off the live WebSocket feed) — no disk-caching path for tokens, transcripts, or logs. |

**One documentation note, not a fix:** `ServerConfig.baseURL`/`webSocketURL`
(`macos/Lopi/Store/ServerConfig.swift:11-17`) are hardcoded `http://`/`ws://`. That's fine
against the default loopback host — row 4's ATS default should block a non-loopback `http://`
target outright — but if a user ever repoints `host` at a non-loopback address (e.g. iOS
talking to a Mac over LAN) and ATS's loopback exemption doesn't apply the way assumed here,
the `Authorization: Bearer` header would travel in cleartext. Worth a one-line comment in
`ServerConfig.swift` for the next person who touches it; not a code change this sprint since
the reachable case is exactly the one ATS already appears to block by default.
