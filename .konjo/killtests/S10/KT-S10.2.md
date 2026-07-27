# KT-S10.2 — does coupling permission mode to source trust cost too much to ship?

**Sprint:** S10, Phase 3 · **Verdict:** structural coupling shipped; live benchmark not run
— environment substitution, named rather than hidden.

**File under test:** `crates/lopi-core/src/permission_mode.rs`
(`effective_permission_mode`), `crates/lopi-agent/src/runner/run_loop.rs`.

## Method

The brief asks for the T01–T10 benchmark corpus (`benchmarks/corpus/`) run under the
strictest permission mode that still completes, measuring pass rate and wall-clock
against baseline. That corpus requires a live `claude` CLI with real subscription
authentication and actual repository/hardware access
(`.claude/rules/benchmarking.md`: an attended, hardware-required action) — this
sprint's environment has neither. Per this repo's own `decays: state` /
"environment substitution" convention (`.konjo/killtests/F3/KT-3.1.md` sets the
precedent: substitute a synthetic harness for an unavailable live requirement rather
than skip the check or fabricate a result), this kill-test substitutes:

1. **The structural claim** — an untrusted-sourced task cannot request
   `BypassPermissions` and get it — verified live via unit tests, not simulated:
   `crates/lopi-core/src/permission_mode.rs`'s
   `untrusted_source_downgrades_bypass_permissions_to_dont_ask` and
   `untrusted_source_downgrades_every_requested_mode_to_dont_ask` construct real
   `TaskSource::Webhook`/`Telegram` values and a real `PermissionMode` and assert the
   downgrade, for every one of the four requestable modes.
2. **The non-interference claim** — trusted sources are unaffected —
   `trusted_sources_pass_the_requested_mode_through_unchanged` asserts the requested
   mode passes through verbatim for `Cli`/`Api`/`SelfModify`/`SelfAuthored`.

```
$ cargo test -p lopi-core --lib permission_mode
running 8 tests
test permission_mode::tests::default_is_bypass_permissions ... ok
test permission_mode::tests::round_trips_as_str_for_every_variant ... ok
test permission_mode::tests::as_str_matches_the_cli_literal_exactly ... ok
test permission_mode::tests::unknown_mode_names_itself_in_the_error ... ok
test permission_mode::tests::is_case_sensitive_not_coerced ... ok
test permission_mode::tests::serializes_to_the_cli_literal_not_snake_case ... ok
test permission_mode::tests::untrusted_source_downgrades_bypass_permissions_to_dont_ask ... ok
test permission_mode::tests::untrusted_source_downgrades_every_requested_mode_to_dont_ask ... ok
test permission_mode::tests::trusted_sources_pass_the_requested_mode_through_unchanged ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Not measured, named as a gap

**Pass rate and wall-clock under `DontAsk` vs. baseline, on the real T01–T10 corpus,
against a live `claude` subscription.** This is a genuine unknown this kill-test does
not resolve: `DontAsk` ("only pre-approved commands run, else denied") could plausibly
stall an untrusted-sourced task's ordinary CI-investigation work (reading logs,
re-running a failing test) if lopi's own `permission_allow` list doesn't already cover
the commands a CI-fix task typically needs. No live measurement of that friction exists
in this sprint.

## Verdict

Per the brief's own explicit escape hatch — **"If KT-S10.2 shows strict mode is too
costly, ship the coupling anyway and leave the global default."** — the coupling ships
regardless of the unmeasured cost, since it's a narrower control (only untrusted-sourced
tasks are affected; the global default, `BypassPermissions`, is unchanged for trusted
tasks) and "a narrower control that stays on beats a broader one operators disable."
This kill-test's honest status is: the safety property is proven; the cost is not
measured. Both facts are recorded, not just the convenient one.

Follow-up (not this sprint): run the real T01–T10 corpus under `DontAsk` for a
webhook-sourced task in an attended session with `permission_allow` tuned for the
common CI-investigation commands (`cargo test`, `git log`, `gh run view`, etc.), and
compare completion rate against the `BypassPermissions` baseline.
