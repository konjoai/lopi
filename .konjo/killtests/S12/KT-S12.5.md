# KT-S12.5 — can `POST /api/tasks` set posture fields the UI doesn't expose?

**Sprint:** S12, Phase 5 · **Verdict:** PASS (the fields are accepted) / no bypass found —
verified, not a fix, per the reasoning below.

**File under test:** `crates/lopi-ui/src/web/handlers.rs` (`create_task`, `apply_loop_fields`),
`crates/lopi-ui/src/web/types.rs` (`CreateTaskRequest`), `crates/lopi-core/src/permission_mode.rs`
(`effective_permission_mode`), `crates/lopi-ui/src/web/mod.rs` (route/auth wiring).

## Method

Per the brief: "construct a `POST /api/tasks` body setting `permission_mode` and the highest
autonomy level directly. If it is accepted, an authenticated-but-lower-privilege path can
escalate the loop's posture — and after S10 Phase 3 couples posture to source trust, that
becomes a bypass of the coupling."

Implemented as `create_task_accepts_posture_fields_but_provenance_stays_operator`
(`crates/lopi-ui/src/web/tests.rs`):

1. POST a body setting `permission_mode: "bypassPermissions"`, `gate`/`until` (repo-supplied
   shell guard commands — the S10 Phase 0 concern, not just posture), and (adversarially) a
   `source` field attempting to masquerade as `TaskSource::Webhook` — even though
   `CreateTaskRequest` has no such field, to prove an unknown field can't sneak through.
2. Confirm `201 Created`.
3. `GET /api/tasks/:id` and confirm `provenance` is `"operator"` — not influenced by anything
   in the request body.

```
$ cargo test -p lopi-ui create_task_accepts_posture_fields --lib
running 1 test
test web::tests::create_task_accepts_posture_fields_but_provenance_stays_operator ... ok
```

## Findings

1. `CreateTaskRequest` **does** accept `permission_mode`, `gate`, and `until` directly — none
   of these are UI-only. `autonomy_level` is not client-settable via this endpoint; it isn't a
   field on `CreateTaskRequest` at all (`crates/lopi-ui/src/web/types.rs`), though a sibling
   endpoint, `POST /api/schedules/:id/autonomy`, does set it directly for scheduled tasks.
2. `CreateTaskRequest` has **no `source` field** — `create_task` always constructs
   `Task::new(...)`, which hardcodes `source: TaskSource::Cli` (`crates/lopi-core/src/task.rs:401`).
   An extra `"source"` key in the JSON body is silently ignored by serde (no
   `deny_unknown_fields`), and has no effect either way.
3. `effective_permission_mode` (S10 Phase 3's coupling) and `resolve_guard_command` (S10 Phase 0)
   both key their trust decision on `task.source`, not on whether the request merely supplied
   `permission_mode`/`gate`/`until`. Since (2) means `task.source` is always `Cli` for anything
   built by this handler, the coupling holds regardless of what posture fields the body sets.
4. Every route under `/api/*`, including `POST /api/tasks`, sits behind the same
   `auth_middleware`/`rate_limit_middleware` pair applied once to the whole router
   (`crates/lopi-ui/src/web/mod.rs:296-303`) — confirmed by reading the router construction, not
   assumed. There is no route in this list that skips auth.

## Verdict

**No bypass reproduces.** The brief's hypothesis — "an authenticated-but-lower-privilege path"
— does not have a referent in lopi's current, single-operator trust model: every caller that
can reach this bearer-token-gated endpoint has already proven possession of the operator's own
credential, which *is* the trust boundary (see `SECURITY.md`'s "Deployment model", written this
sprint). There is no second, lesser-privileged, still-authenticated principal for a request
body to escalate out of. The UI choosing not to expose a field is a UX decision, not a security
boundary, when the UI and the raw API share one authenticated principal.

This is a genuinely different outcome than a multi-tenant system would have — there, "the UI
doesn't expose it" often *is* the only thing standing between a low-privilege tenant and a
high-privilege posture, which is exactly why Phase 0's scope lock matters here: it's what makes
this verdict correct rather than wishful.

**Not re-litigated by this kill-test, and still true:** an *untrusted-source* task (webhook/PR)
correctly cannot reach elevated posture through this path at all — it never reaches `create_task`
in the first place; it's constructed directly in trusted Rust code by `lopi-webhook`
(`queue_ci_fix`/`handle_pr_review`/issue triage), which sets `TaskSource::Webhook`/`Telegram`
explicitly and is unreachable from attacker-controlled JSON. See
`docs/security/TRIFECTA_PATHS.md` §6 for that path's own gating.

**Related, separately tracked gap (not this kill-test's scope):** `docs/security/TRIFECTA_PATHS.md`
§7 row 5 records that the MCP tool `lopi_submit_task` — a different task-creation path, reachable
from inside an agent's own tool calls rather than over HTTP — has no equivalent trust check at
all. That is a real, named, unfixed gap; it just isn't the `POST /api/tasks` mass-assignment
scenario this kill-test set out to reproduce.
