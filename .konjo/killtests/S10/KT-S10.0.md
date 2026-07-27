# KT-S10.0 — can a pull request execute shell via `.lopi/loop.toml`? (BLOCKING)

**Sprint:** S10, Phase 0 · **Verdict:** PASS (pre-fix) / FAIL (post-fix) — the pre-fix
pass is the finding; the post-fix failure is the remediation proof.

**File under test:** `crates/lopi-core/src/loop_config.rs` (`run_guard_command`,
`resolve_guard_command`), `crates/lopi-orchestrator/src/pool/run_loop.rs` (`run_one`).

## Method

Per the brief: "A branch that adds `.lopi/loop.toml` with a `gate` command writing a
marker file. Queue a task against it through the webhook path... Write to disk. Do not
point it at a network endpoint."

Implemented as `kt_s10_0_webhook_sourced_task_cannot_execute_repo_supplied_gate`
(`crates/lopi-core/src/loop_config_tests.rs`) — an in-process reproduction using the
exact production call sequence `run_one` uses, against a real repo directory on disk
(no HTTP layer needed to prove the finding; "write to disk, don't point it at a network
endpoint" is exactly what this test does):

1. Write `.lopi/loop.toml` to a temp directory with `gate = "touch <marker>"` — as if a
   pull request under evaluation added this file.
2. `LoopConfig::load_from_repo(&dir)` — the real production loader.
3. Build a `TaskSource::Webhook { repo: "attacker/repo", event: "pull_request" }` — what
   `lopi-webhook`'s `queue_ci_fix`/`handle_pr_review` actually construct.
4. `resolve_guard_command(cfg.gate.as_deref(), None, !is_untrusted_source(&source))` —
   the Phase 0 fix.
5. Only if `resolve_guard_command` returns `Some` does the test call
   `run_guard_command` — the actual `sh -c` shell-out.
6. Assert the marker file does not exist.

```
$ cargo test -p lopi-core kt_s10_0 --lib
running 1 test
test loop_config::tests::kt_s10_0_webhook_sourced_task_cannot_execute_repo_supplied_gate ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**Pre-fix reconstruction** (reverting `resolve_guard_command`'s call in `run_one` to a
bare `cfg.gate.clone()`, matching the code at baseline `a384f32`): the same test, with
step 4 removed (`run_guard_command` called directly on `cfg.gate`), creates the marker
file unconditionally — confirmed by temporarily inlining that version of the test during
development. Severity confirmed exactly as the brief predicted.

## Verdict

**Pre-fix: PASS (the marker is created) — this is the finding, and it is the expected,
severity-confirming result per the brief's own framing.** A repository-controlled
`.lopi/loop.toml`, reachable via any webhook-dispatched task (`TaskSource::Webhook`),
executes an attacker-chosen shell command with no gate, no approval, no signature check
— before any human ever sees a plan (`run_gate_preflight` runs before `gate_plan`/
plan-approval in `AgentRunner::run()`).

**Post-fix: FAIL (the marker is never created)** — `resolve_guard_command` refuses the
repo-supplied `gate` value because the task's source is untrusted and no operator
override (`~/.lopi/loop.toml`) is configured. The same mechanism gates `until` and
`test_command` (also `LoopConfig`-sourced) via the same call in `run_one`, and
`Task.acceptance`'s `Shell`/`Suite` checks via a parallel `EvalContext.shell_commands_trusted`
gate in `crates/lopi-agent/src/eval/tiers.rs` (`shell_and_suite_tiers_refuse_when_untrusted`).

Not covered by this kill-test, named rather than implied: the exact ordering by which a
malicious PR branch's `.lopi/loop.toml` becomes visible to a *specific* running task's
working directory (whether via `Branch` or `Worktree` isolation, and at what point in the
agent's own tool-use the branch gets checked out) was not fully traced against every
isolation mode this sprint. The fix does not depend on that ordering — it refuses the
repo-supplied value based purely on task-source trust, regardless of when or whether the
malicious file becomes visible on disk — but a full TOCTOU trace of the worktree
lifecycle remains an explicitly named gap in the parent audit (see the sprint brief's own
"not covered" list).
