# KT-S12.1 — does a secret in agent stdout reach `task_logs` and the SSE stream unredacted?

**Sprint:** S12, Phase 1 · **Verdict:** PASS (pre-fix) / FAIL (post-fix) — the pre-fix pass is
the finding; the post-fix failure is the remediation proof.

**File under test:** `crates/lopi-ui/src/web/event_bridge.rs` (the bridge that fans an
`AgentEvent::LogLine` out to both `task_logs` persistence and the live broadcast),
`crates/lopi-core/src/redact.rs` (the fix).

## Method

Per the brief: "run a task on a repo whose test output prints a fake secret in several
shapes — `sk-ant-…`, `ghp_…`, `AWS_SECRET_ACCESS_KEY=…`, a JWT, a `postgres://user:pass@host`
URL. Confirm each reaches `task_logs` and the SSE stream."

Implemented as `secret_in_log_line_is_redacted_before_persist_and_broadcast`
(`crates/lopi-ui/src/web/event_bridge.rs`'s test module) plus unit coverage of every shape
named in the brief in `crates/lopi-core/src/redact.rs`'s test module
(`anthropic_key_is_redacted`, `github_pat_is_redacted`, `aws_secret_env_assignment_is_redacted`,
`jwt_is_redacted`, `postgres_url_with_credentials_is_redacted`, plus `bearer_header` as a bonus
shape):

1. Send an `AgentEvent::LogLine` through the real `event_bridge::spawn` path (the exact
   production wiring `AppState::new_with_repo` uses) whose text contains
   `sk-ant-api03-abcdefghijklmnopqrstuvwxyz0123456789`.
2. Read the value the live broadcast channel actually emits (what an SSE subscriber would
   receive).
3. Wait for the batched drain task to flush, then read the row back from the real
   `task_logs` SQLite table via `MemoryStore::load_task_logs`.

```
$ cargo test -p lopi-ui secret_in_log_line --lib
running 1 test
test web::event_bridge::tests::secret_in_log_line_is_redacted_before_persist_and_broadcast ... ok

$ cargo test -p lopi-core redact --lib
running 9 tests
test redact::tests::anthropic_key_is_redacted ... ok
test redact::tests::github_pat_is_redacted ... ok
test redact::tests::aws_secret_env_assignment_is_redacted ... ok
test redact::tests::jwt_is_redacted ... ok
test redact::tests::postgres_url_with_credentials_is_redacted ... ok
test redact::tests::bearer_token_is_redacted ... ok
test redact::tests::multiple_secrets_in_one_line_are_all_redacted ... ok
test redact::tests::benign_line_is_untouched_and_unallocated ... ok
test redact::tests::patterns_file_parses_without_panicking ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**Pre-fix reconstruction:** before this sprint, `grep -rn "redact|scrub|sanitiz"` across
`crates/lopi-agent/src/` returned only the env-scrub function (`scrub_inherited_anthropic_env`)
— nothing on the `AgentEvent::LogLine` → `task_logs`/broadcast path. Temporarily removing the
`redact_log_line` call in `event_bridge.rs`'s bridge loop reproduces the pre-fix behavior: the
same test's assertions (`!broadcast_json.contains("sk-ant-")`, `!rows[0].line.contains("sk-ant-")`)
fail, confirming the secret reached both sinks verbatim.

## Verdict

**Pre-fix: PASS (the secret is present in both the broadcast JSON and the persisted row) —
this is the finding.** Any value an agent's own stdout contains — a `cat` of a `.env`, a build
script echoing a variable, a test failure printing a connection string — reached `task_logs`
and the live SSE/WS stream with nothing on that path ever redacting it, regardless of whether
S11 Phase 0's SSE-authentication work has landed.

**Post-fix: FAIL (the secret is absent, replaced by `[REDACTED:<label>]`, in both sinks)** —
`lopi_core::redact::redact_secrets` is called once, in `event_bridge.rs`'s bridge loop, before
the event fans out to either sink, so persistence and broadcast cannot drift out of sync with
each other.

**Named limit, not silently omitted:** this is pattern-based redaction against known secret
shapes (`crates/lopi-core/redact_patterns.txt`). It will miss a bespoke internal token format,
a secret split across two log lines, or an unusual encoding. It is a mitigation, not a
guarantee — the doc comment on `redact_secrets` says so explicitly, and S11 Phase 0's SSE
authentication (once landed) remains the actual control against an unauthenticated subscriber,
not this redaction pass.
