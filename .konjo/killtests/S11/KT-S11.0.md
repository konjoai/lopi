# KT-S11.0 — is the live event stream reachable with no auth? (BLOCKING)

**Sprint:** S11 Round 2, Phase 0 · **Verdict:** PASS (pre-fix) / FAIL (post-fix) — the
pre-fix pass is the finding; the post-fix failure is the remediation proof.

**Files under test:** `crates/lopi-ui/src/web/mod.rs` (`build_app`), `crates/lopi-ui/src/web/api_middleware.rs`
(`auth_middleware`, `rate_limit_middleware`), `crates/lopi-ui/src/web/streaming.rs`
(`sse_handler`, `ws_handler`, `handle_ws`), `crates/lopi-ui/src/web/metrics_handlers.rs`
(`metrics`).

## Method

Exactly the brief's method: build the real binary at baseline, start it with a real
`auth_token` configured (`LOPI_WEB_AUTH_TOKEN`, loopback bind, no `--insecure-no-auth`),
and hit `/sse`, `/metrics`, `/ws` with **no** `Authorization` header — first to confirm
the vulnerability against the unmodified baseline, then again after the fix.

```bash
d=$(mktemp -d) && (cd "$d" && git init -q && git config user.email t@t.com \
  && git config user.name t && echo hi > f.txt && git add -A && git commit -qm init)
LOPI_WEB_AUTH_TOKEN=secret-test-token target/debug/lopi sail --port 3911 \
  --host 127.0.0.1 --repo "$d" &
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:3911/api/health   # sanity: auth is live
```

`websocat` was unavailable in this environment; `/ws` was probed with `curl`'s manual
WebSocket handshake (`Connection: Upgrade`, `Upgrade: websocket`, a valid
`Sec-WebSocket-Key`/`Sec-WebSocket-Version`) instead — `curl -i` prints the raw
`HTTP/1.1 101 Switching Protocols` response line and streams the frames that follow on
`-N`, which is sufficient to prove the same thing `websocat` would: the socket upgrades
and streams with zero credentials.

## Pre-fix (baseline `a384f32`, i.e. the code before this sprint's Phase 0 changes)

```
$ curl -s -N --max-time 2 -D - http://127.0.0.1:3911/sse
HTTP/1.1 200 OK
content-type: text/event-stream
data: {"type":"pool_stats","running":0,"queued":0,"succeeded":0,"failed":0,"uptime_secs":9}
data: {"type":"pool_stats","running":0,"queued":0,"succeeded":0,"failed":0,"uptime_secs":10}

$ curl -s -D - http://127.0.0.1:3911/metrics
HTTP/1.1 200 OK
content-type: text/plain; version=0.0.4
# HELP lopi_agents_running Currently running agents
...(full metrics body, 1111 bytes)...

$ curl -s -i --max-time 2 -H "Connection: Upgrade" -H "Upgrade: websocket" \
    -H "Sec-WebSocket-Version: 13" -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
    http://127.0.0.1:3911/ws
HTTP/1.1 101 Switching Protocols
connection: upgrade
upgrade: websocket
{"stats":{"failed":0,"queued":0,"running":0,"succeeded":0,"uptime_secs":11},"tasks":[],"type":"snapshot"}
{"type":"pool_stats","running":0,"queued":0,"succeeded":0,"failed":0,"uptime_secs":12}
{"type":"pool_stats","running":0,"queued":0,"succeeded":0,"failed":0,"uptime_secs":13}
```

**Pass (as predicted) — this is the finding.** All three streamed in full with no
`Authorization` header at all, while `/api/health` on the same server correctly returned
`401` in the same run — proving the auth token *was* configured and active, just not
applied to these three routes. `/ws` sends the full snapshot (last 100 tasks, per-task
cost, status counts) before a client has proven anything about who it is. Severity
confirmed exactly as the brief described.

## Fix

`build_app` (`mod.rs`) previously built an `api` router scoped to `/api/*`, applied
`rate_limit_middleware`/`auth_middleware` to it via `route_layer`, then `.merge()`d it
into an *outer* `Router` that registered `/metrics`, `/sse`, `/ws`, `/ws/tasks` **after**
the merge — outside the layer those four calls happened to sit next to.

The fix is structural, not "add a check to four routes": `/metrics`, `/sse`, `/ws`,
`/ws/tasks` now live inside the same `protected` router as every `/api/*` route, so they
share its `route_layer` calls by construction. The outer router now registers exactly one
thing — the static/SPA `fallback` — as the single, explicit, named public surface. A new
route added to `protected` inherits auth automatically; the only way to add an
unauthenticated route is to add it to the outer router's explicit allowlist, which is one
line next to a comment explaining why.

`/ws`, `/ws/tasks`, `/sse` additionally accept a single-use, 30-second ticket
(`?ticket=<value>`, minted by an already-authenticated `POST /api/ws-ticket`) as an
alternative to the `Authorization` header, because browsers cannot set custom headers on
a `WebSocket`/`EventSource` upgrade. `/metrics` does **not** accept a ticket — a
Prometheus scraper can set an `Authorization` header like any other HTTP client, so it
gets the same credential every other `/api/*` caller uses, per the brief's remediation
note 4 ("give it its own credential... don't leave it open because scraping is
inconvenient").

## Post-fix

```
$ curl -s -o /tmp/sse-postfix.out -D - --max-time 2 http://127.0.0.1:3912/sse
HTTP/1.1 401 Unauthorized
content-type: application/json
{"error":"unauthorized"}

$ curl -s -D - http://127.0.0.1:3912/metrics
HTTP/1.1 401 Unauthorized
{"error":"unauthorized"}

$ curl -s -i --max-time 2 -H "Connection: Upgrade" -H "Upgrade: websocket" \
    -H "Sec-WebSocket-Version: 13" -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
    http://127.0.0.1:3912/ws
HTTP/1.1 401 Unauthorized
{"error":"unauthorized"}

$ curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:3912/ws/tasks
401
```

**Fail (the intended, remediation-confirming result).** No connection reaches the
handler; no snapshot, no metrics body, no upgrade.

The two legitimate credentialed paths were also verified live against the same fixed
binary:

```
$ TICKET=$(curl -s -X POST -H "Authorization: Bearer secret-test-token" \
    http://127.0.0.1:3912/api/ws-ticket | jq -r .ticket)
$ curl -s -o /dev/null -w "%{http_code}\n" "http://127.0.0.1:3912/sse?ticket=$TICKET"
200
$ curl -s -o /dev/null -w "%{http_code}\n" "http://127.0.0.1:3912/sse?ticket=$TICKET"
401   # replay of the same ticket — single-use, confirmed
$ curl -s -i --max-time 2 -H "Authorization: Bearer secret-test-token" \
    -H "Connection: Upgrade" -H "Upgrade: websocket" \
    -H "Sec-WebSocket-Version: 13" -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
    http://127.0.0.1:3912/ws
HTTP/1.1 101 Switching Protocols
```

Automated coverage (`cargo test -p lopi-ui --lib web::`, 209 passed): `sse_without_token_is_401`,
`metrics_without_token_is_401`, `ws_without_token_is_401_before_any_upgrade_is_attempted`,
`ws_tasks_legacy_alias_without_token_is_401`, `ws_ticket_mint_itself_requires_the_real_bearer_token`,
`a_minted_ticket_lets_sse_through_exactly_once`, `an_unknown_ticket_is_rejected`,
`tickets_are_not_accepted_on_metrics`, `tickets_are_not_accepted_on_plain_api_routes`,
`correct_bearer_still_works_on_streaming_routes_no_ticket_needed`,
`streaming_routes_pass_through_untouched_when_no_auth_token_is_configured` (all in
`crates/lopi-ui/src/web/streaming_auth_tests.rs`), plus the four `ws_ticket::tests` unit
tests (`crates/lopi-ui/src/web/ws_ticket.rs`) and the route-coverage gate — see KT
below and Phase 4 of `CHANGELOG.md`.

## Route-coverage gate (Phase 4 requirement, verified as part of this kill-test)

`crates/lopi-ui/src/web/route_coverage_tests.rs` enumerates every path `build_app`
registers on `protected` (49 entries, one per distinct path) plus the explicit public
allowlist (`/`, `/favicon.svg`, an unrecognized SPA route), and asserts: every protected
entry 401s with no token and accepts the correct Bearer token; every public entry never
401s even when an auth token is configured. This is the test that would have caught
Phase 0's bug directly — it doesn't check "does auth work on the routes someone
remembered to test", it walks the same route list `build_app` builds from and would show
a route silently missing from `PROTECTED_ROUTES` (and therefore uncovered) as an
obviously-wrong accounting, not a green test.

**Known limitation, named rather than hidden:** axum 0.7 has no public API to introspect
a `Router`'s registered paths at runtime, so `PROTECTED_ROUTES` is a hand-maintained list
kept in lockstep with `build_app`, not derived from it automatically. A route added to
`build_app` and *not* added to this list would not be caught by this gate — the list's
own doc comment says so. This is the same class of limitation the brief's own audit
lesson (§Corrections, Phase 4) names: a coverage claim is only as good as what it actually
walks.

## Not covered by this kill-test

The `/api/*` bearer-auth path itself (pre-existing, `auth_rejects_missing_token` et al.)
was not re-derived here — only the four routes this sprint's Phase 0 flagged, plus the new
ticket mechanism they needed. Whether the web dashboard's own `fetch()` calls
(`web/src/lib/api.ts`) ever attach an `Authorization` header — they do not, today, checked
during this sprint but out of Phase 0's named scope — is a pre-existing gap that makes the
SPA itself non-functional against a server with an `auth_token` configured, independent of
anything this kill-test changed; see `LEDGER.md` for why this is named and not fixed here.
