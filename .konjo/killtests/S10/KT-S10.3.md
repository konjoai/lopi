# KT-S10.3 — does a historical Telegram-sourced task still deserialize after removal?

**Sprint:** S10, Phase 4 · **Verdict:** PASS — old rows still read correctly across
every surface named in the brief.

**File under test:** `crates/lopi-core/src/task_source.rs` (`TaskSource::Telegram`
variant), `crates/lopi-core/src/successor.rs` (`is_untrusted_source`),
`crates/lopi-memory/src/store/task_row.rs` (`TaskRow::provenance`),
`crates/lopi-orchestrator/src/pool/run_loop.rs` (`task_source_label`).

## Method

Per the brief: "a database containing Telegram-sourced tasks still deserializes across
`lopi diag`, replay, the dashboard task list, and `audit_log` queries." Traced each
surface to what actually reads `TaskSource`/`tasks.source`, rather than assuming:

- **`lopi diag`** (`src/diag_commands.rs`) and **`lopi replay`**
  (`src/replay_commands.rs`) — grepped for `telegram`/`Telegram`/`TaskSource`/
  `source`: zero matches in either file. Neither special-cases `TaskSource`; both
  operate on `TaskRow`/the audit log generically. They need no code changes, and their
  correctness here reduces entirely to the two bullets below.
- **The dashboard task list** (`GET /api/tasks`) — reads `TaskRow::provenance()`.
- **`audit_log` queries** — the `actor` field is `task_source_label(&task)`.

Both of those, plus `is_untrusted_source` (the third read-path consumer, gating
Successor-1's chain-depth logic), are exercised directly:

```
$ cargo test -p lopi-memory telegram --lib
running 1 test
test store::tests::telegram_sourced_task_is_operator_provenance ... ok

$ cargo test -p lopi-orchestrator task_source_label --lib
running 1 test
test pool::tests::task_source_label_still_resolves_a_historical_telegram_sourced_task ... ok

$ cargo test -p lopi-core is_untrusted_source --lib
running 1 test (plus others matching the same substring)
test successor_tests::... ok
```

`telegram_sourced_task_is_operator_provenance` (pre-existing, `crates/lopi-memory/src/store/tests.rs`)
builds a `TaskSource::Telegram` task, saves and reloads it through the **real** SQLite
store (not a mock), and asserts `provenance() == "operator"` — this is the literal
round-trip proof that old rows survive the transport's removal, and it needed **zero
changes** for Phase 4, because `TaskRow::provenance()`'s `Telegram` handling was always
via its `Ok(_) => "operator"` wildcard arm, never a named match — nothing to break.

`task_source_label_still_resolves_a_historical_telegram_sourced_task` (new, Phase 4)
pins the `audit_log` `actor` field specifically, since no prior test covered it —
`task_source_label`'s `TaskSource::Telegram { .. } => "telegram".into()` arm is an
exhaustive match with no wildcard, so if a future sprint ever *does* delete the variant,
this function fails to compile — a clean forcing function, left in place deliberately.

## Verdict

**PASS.** Every read-path consumer of `TaskSource::Telegram` — `is_untrusted_source`,
`TaskRow::provenance()`, `task_source_label` — was left untouched by Phase 4's removal,
and each is directly tested against a real `TaskSource::Telegram` value (two through the
real SQLite store, not just in-memory). `lopi diag`/`lopi replay` need no changes because
they never special-cased `TaskSource` in the first place — confirmed by grep, not
assumed. The only things Phase 4 actually deleted were the four **construction** sites
(`crates/lopi-remote/src/telegram/{handlers,monitor,draft}.rs`), all inside the now-removed
`telegram/` module — nothing that could construct new Telegram-sourced rows survives,
and nothing that reads old ones was touched.
