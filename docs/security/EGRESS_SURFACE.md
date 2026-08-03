---
decays: state
verified-against: 6919a1d
verified-date: 2026-08-03
---

## Sprint S10, Phase 4 update (2026-07-27)

**The transport this document is about no longer exists.** Sprint S10, Phase 4 removed the
Telegram bot (`crates/lopi-remote/src/telegram/`, ~2,024 LOC) and, with it, `notify_loop` and
`crates/lopi-remote/src/egress.rs` (its sole caller). §1's "exactly one live outbound transport
(Telegram), and it is already gated" conclusion is now moot — there is currently **no live
outbound transport at all** (WhatsApp remains inbound-only, per §1's own second row, unchanged).
`TaskSource::Telegram` itself is not removed (durable enum, historical rows still deserialize —
see `docs/security/TRIFECTA_PATHS.md` §1 row E and `LEDGER.md`), so §2's destination-provenance
trace and §4's provenance-marker work remain historically accurate for reading old data; they are
no longer live code paths for new sends. `egress.rs`'s deny-by-default allowlist *shape* was
reused, not deleted-and-forgotten: Sprint S10 Phase 5 (`crates/lopi-mcp/src/allowlist.rs`) mirrors
it for a different chokepoint — which MCP servers `McpServerSpec::connect` may spawn — since that
module's only caller was gone anyway. See `docs/security/TRIFECTA_PATHS.md` §6 for the current
(Sprint S10) untrusted-input inventory. Left below as historical record rather than deleted,
consistent with this repo's `decays: state` convention — re-derive before trusting any of it.

# Egress surface — the local-only remnant of Sprint S2

Verified against: `6919a1d` · 2026-08-03 (re-verified; Sprint P0's commit volume on this
PR pushed this past the 20-commit cap again, not on anything it cites losing accuracy.
P0 is a pure Rust logic addition (`crates/lopi-core/src/cost_breaker.rs`, a token-count
ceiling check with no I/O) plus config/docs changes; none of this doc's cited files
(`lopi-ui::web::handlers`, `provenance_field_tests.rs`, `lopi-memory::store::tests`,
`whatsapp.rs`) were touched, and P0 adds no new outbound transport of any kind. Prior
banner (`1dd471d` · 2026-07-29, re-verified again; Sprint S13R's own 9-commit
volume pushed this past the 20-commit cap again, not on anything it cites losing
accuracy. None of its cited files (`lopi-ui::web::handlers`, `provenance_field_tests.rs`,
`lopi-memory::store::tests`) changed this sprint; `whatsapp.rs`'s only S13R edit named an
existing dev-mode signature bypass as an explicit override, same "inbound-only, no
outbound send call anywhere in the file" fact this doc's §1 second row already states,
re-confirmed with the same grep this sprint. Earlier banner (`28dd4cf` · 2026-07-28,
re-verified again; this doc keeps crossing
the 20-commit staleness cap purely on Sprint E/Finding #10's own merge-commit volume
(two reconciliations with `main`), not on anything it cites losing accuracy.
Re-checked the same citations as the prior (`8cc1694`) pass, since none of the
intervening commits (the symbol-index sprint, this sprint's own reservation-cleanup
wiring) touched `lopi-ui::web::handlers`, `provenance_field_tests.rs`, or
`lopi-memory::store::tests`: `GET /api/tasks`/`GET /api/tasks/:id` still serialize
`"provenance"` unchanged (`crates/lopi-ui/src/web/handlers.rs:86,114` —
`t.provenance()`, exactly as §4 describes). `TaskRow::provenance()`,
`get_task_surfaces_provenance_marker`
(`crates/lopi-ui/src/web/provenance_field_tests.rs`),
`operator_and_untrusted_sources_have_distinguishable_provenance`, and
`telegram_sourced_task_is_operator_provenance` (both `crates/lopi-memory/src/store/tests.rs`)
all still exist exactly as cited. No other citation re-checked this round — see the
2026-07-27 pass below for the last full re-derivation.)

This is the pre-flight kill-test for Sprint S2′ ("Egress allowlist: bound the one
trifecta leg that's still open locally"). The sprint brief cited a baseline of
`3a8a2ff` (v0.24.0) and framed the deny-by-default egress allowlist as still open.
**Re-deriving against the actual current `main` found that baseline stale**: the full
Sprint S2 (`docs/security/TRIFECTA_PATHS.md`, PR #157, merged as `34a73d1`) had already
shipped by the time this sprint started, and its Phase 4 is exactly this sprint's
Phase 1 — a deny-by-default egress allowlist, already in `crates/lopi-remote/src/egress.rs`.
`decays: state` — this document itself will go stale the same way; re-run this
kill-test before trusting it in a later sprint, the same instruction `TRIFECTA_PATHS.md`
gives about itself.

## 1. Transport inventory

| Transport | Direction | Outbound send exists? | Egress-gated? |
|---|---|---|---|
| Telegram | in + out | Yes — `notify_loop` (`telegram/notify.rs`), completion notifications, report-on-finish, budget alerts | **Yes** — `crates/lopi-remote/src/egress.rs::check_egress`, deny-by-default, checked once at `notify_loop`'s entry before the event loop starts (covers every message type that loop ever sends, since they all share the one gated `chat_id`) |
| WhatsApp | in only | **No** — `crates/lopi-remote/src/whatsapp.rs` is a Twilio inbound webhook handler only (`/webhook/whatsapp` → parses `/task <goal>` → pushes to `TaskQueue`). Confirmed via `grep -rn "bot.send\|Client::new\|twilio.*send" crates/lopi-remote/src/whatsapp.rs` — no match. There is no code path in this repository that sends a WhatsApp message out. | N/A — nothing to gate. Flagged in `TRIFECTA_PATHS.md` §4 as dormant (`whatsapp::serve` isn't called from any CLI command either); if an outbound WhatsApp sender is added later, it needs the same deny-by-default treatment `egress.rs` gives Telegram before it ships, not after. |
| Raw webhook-forward / email / generic HTTP notifier | — | **No such transport exists.** `grep -rln "reqwest::Client\|ureq::" crates/lopi-remote/src crates/lopi-webhook/src` finds no outbound HTTP notifier distinct from the two above. | N/A |

**Conclusion: exactly one live outbound transport (Telegram), and it is already gated.**
No third transport was found that this sprint (or the merged S2) would have missed.

## 2. Destination-provenance trace

Traced every `bot.send_message` call site in `crates/lopi-remote/src/telegram/`:

- **Proactive/automated sends** — `notify_loop` (`notify.rs`) and everything it calls
  (`handle_event`, `route_report_ready`, `send_msg`) all target the single `chat_id`
  the loop was booted with: an operator-configured value from `lopi.toml`'s
  `[remote.telegram].chat_id`, never derived from task content, webhook payloads, or
  agent output. This is the leg `egress_allowed_chat_ids` gates.
- **Reply sends** — every other `bot.send_message` call site (`handlers.rs`,
  `monitor.rs`, `budget.rs`, `draft.rs`, `callbacks.rs`) targets `msg.chat.id` /
  `msg.chat().id`: the chat that sent the inbound command. These chats already passed
  `allowed_chat_ids` inbound authz (`telegram/mod.rs`'s `message_handler` /
  `text_message_handler` gate, and — since Sprint S2 — `callback_query_handler` too).
  A reply can never reach an unauthorized destination because it never leaves the
  chat that got past the front door; this is why `egress.rs`'s own module doc
  deliberately keeps this allowlist separate from (and default-*closed*, vs.
  `allowed_chat_ids`'s default-*open*) the inbound one.

**No path derives a `chat_id`, phone number, or URL from task data, agent output, or
webhook content.** The classic trifecta exfiltration move — "have the agent's own
output redirect the send" — has no code path to exploit today. Confirmed unchanged
from `TRIFECTA_PATHS.md`'s own §1/§4 findings.

## 3. Local deployment / bind-address re-check

`src/cli.rs`'s `Sail` and `ServeWebhooks` subcommands still default `--host` to
`127.0.0.1` (unchanged since `TRIFECTA_PATHS.md` §0). `fly.toml` **still exists** in
this repository and still runs `lopi sail --host 0.0.0.0` — the sprint brief's framing
("deployment is now local-only... VPS is parked") describes an operational decision,
not a code change, and the file itself wasn't removed. That is fine: Sprint S2's
Phase 1 (`crates/lopi-ui/src/web/auth_policy.rs::validate_auth_policy`) already closed
the actual exposure this created — `sail`/`serve()` refuse to start on a non-loopback
bind unless auth is explicitly configured, and the `--insecure-no-auth` escape hatch
itself refuses non-loopback. So even with `fly.toml` un-deleted, there's no live gap:
if the VPS path is ever exercised again, it fails closed rather than serving
unauthenticated. No tunnel (ngrok/cloudflared) is part of the documented local
workflow — confirmed via `grep -rln "ngrok\|cloudflared" docs/ README.md scripts/`,
no matches — so the untrusted-webhook-content leg stays shut for a genuinely local
operator, per the sprint brief's own reasoning.

## 4. What this sprint actually changed

Given §1–§3, this sprint's own Phase 1 ask (deny-by-default egress allowlist) needed
**no code change** — already shipped, already tested (`egress.rs`'s
`empty_allowlist_denies_rather_than_permits`, `notify.rs`'s
`denied_egress_destination_never_enters_the_loop`), already documented
(`README.md`'s Security section, `lopi.toml.example`, `CHANGELOG.md`'s `[0.25.0]`
entry). Re-verified rather than re-implemented — this document (and its
`verified-against` stamp) *is* that re-verification.

The one genuine gap the kill-test found: **Phase 2's provenance marker was recorded
but never surfaced.** `Task::source: TaskSource` is serialized into the `tasks.source`
SQLite column on every `save_task` call (this predates this sprint), and
`lopi_core::is_untrusted_source` already classifies `Webhook`/`Telegram` origins as
untrusted for the plan-approval gate — but nothing ever read that column back out.
`MemoryStore::load_history` and `MemoryStore::get_task`'s `SELECT` lists omitted
`source` entirely, and the web API's `list_tasks`/`get_task` handlers never
constructed a provenance field. An operator looking at the dashboard or the JSON API
had no way to see, after the fact, whether a given run came from an authenticated
human action (CLI, API, Telegram) or an unauthenticated webhook. Closed this sprint:

- `TaskRow` (`crates/lopi-memory/src/store/task_row.rs` — split out of `mod.rs` by a
  later, unrelated file-size-gate sprint) gained a `source` field (the raw
  JSON column) and a `provenance()` method returning `"operator"` / `"untrusted"` /
  `"unknown"` (the last only if the column fails to parse — logged, never silent).
  `"untrusted"` matches only `TaskSource::Webhook` — deliberately narrower than
  `lopi_core::is_untrusted_source` (which also flags `TaskSource::Telegram`, for
  Sprint Successor-1's unrelated chain-extension caution). Telegram commands are
  inbound-authenticated via `allowed_chat_ids`; Sprint S2's Phase 5 gate never
  treated them as untrusted either (§2 above, and `TRIFECTA_PATHS.md` §1 row E) —
  this marker mirrors that same operational judgment rather than the broader
  predicate, and has its own regression test
  (`telegram_sourced_task_is_operator_provenance`) pinning it.
- `MemoryStore::load_history` (`mod.rs`) and `MemoryStore::get_task` (`lineage.rs`)
  now `SELECT` the `source` column.
- `GET /api/tasks` and `GET /api/tasks/:id` (`crates/lopi-ui/src/web/handlers.rs`) now
  include `"provenance"` in their JSON response.

**This sprint only records and surfaces the marker — nothing gates on it yet.** The
existing `require_plan_approval` gate (Sprint S2 Phase 5) already gates task
*execution* on `is_untrusted_source`; this marker is the foundation for a future
gate on *notification/egress* specifically (hold a send, not just a plan, when its
originating run was untrusted) — deferred, per the brief, until the VPS/webhook path
returns and that gate is actually load-bearing. See `NEXT_SESSION_PROMPT.md`.

## 5. Verify

- `operator_and_untrusted_sources_have_distinguishable_provenance`
  (`crates/lopi-memory/src/store/tests.rs`) — a `Cli`-sourced task and a
  `Webhook`-sourced task, saved and reloaded through the real store, produce
  `"operator"` and `"untrusted"` respectively.
- `telegram_sourced_task_is_operator_provenance` (same file) — pins the
  deliberate narrowing described above: Telegram must classify as
  `"operator"`, not fall through to the broader `is_untrusted_source`
  predicate.
- `get_task_surfaces_provenance_marker` (`crates/lopi-ui/src/web/provenance_field_tests.rs`
  — split out of `task_field_tests.rs` by a later, unrelated sprint) — same distinction,
  observed through `GET /api/tasks/:id`'s actual JSON response, not just the store layer.
- No behavior change: `cargo test --workspace` and `cargo clippy --workspace -- -D
  warnings` both stay green; nothing that previously succeeded now fails, and nothing
  that previously sent now gets blocked.

Re-run this kill-test before trusting it in a later sprint — `decays: state`.
