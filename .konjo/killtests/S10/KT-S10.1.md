# KT-S10.1 — can a repo read lopi's own credentials via a build script?

**Sprint:** S10, Phase 1 · **Verdict:** PASS (pre-fix) / FAIL (post-fix) — same pattern
as KT-S10.0: the pre-fix pass is the finding.

**File under test:** `crates/lopi-agent/src/claude_support.rs` (`apply_env_allowlist`),
and all five `claude`-CLI spawn sites (`claude_spawn.rs` ×2, `claude_stream.rs`,
`runner/postmortem_cli.rs`, `verifier_cli.rs`).

## Method

Per the brief: "a repo whose build script reads `LOPI_WEB_AUTH_TOKEN` and writes it to
disk. Confirm readable." Implemented as a live child-process spawn (not just `Command`
introspection — `Command::env_clear`'s effect on inherited variables is not observable
via `Command::get_envs()` at all, since that method only ever reports explicit
overrides) in `crates/lopi-agent/src/claude_support_tests.rs`:

`apply_env_allowlist_child_process_cannot_see_a_non_allowlisted_secret`:

1. Set `LOPI_KT_S10_1_SECRET=do-not-leak` and `ANTHROPIC_API_KEY=sk-should-not-leak-either`
   in the test process — standing in for `LOPI_WEB_AUTH_TOKEN`/a real API key the
   operator's shell holds.
2. Build a real `tokio::process::Command::new("env")`, apply `apply_env_allowlist`.
3. Spawn it and capture stdout — the literal environment the child process saw, the
   same shape a malicious build script's `env > /tmp/leaked.txt` would capture.
4. Assert neither secret appears in the child's output; assert `PATH` does (sanity check
   that the allowlist isn't just accidentally empty).

```
$ cargo test -p lopi-agent apply_env_allowlist --lib
running 2 tests
test claude_support::tests::apply_env_allowlist_sets_only_the_allowlisted_vars_present_in_process_env ... ok
test claude_support::tests::apply_env_allowlist_child_process_cannot_see_a_non_allowlisted_secret ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**Pre-fix reconstruction:** before `apply_env_allowlist` existed, every spawn site
called only `scrub_inherited_anthropic_env` (a blocklist of 9 Anthropic-routing +
2 session-identity vars) after full inheritance. `LOPI_KT_S10_1_SECRET` — standing in
for `LOPI_WEB_AUTH_TOKEN`, a configured GitHub token, or anything else not on that
9+2-entry blocklist — was never removed and would appear in the child's `env` output.
Confirmed by inspection: `scrub_inherited_anthropic_env`'s two constant arrays
(`ANTHROPIC_ROUTING_ENV`, `INHERITED_SESSION_ENV`) name every variable it ever removed,
and neither list contains `LOPI_WEB_AUTH_TOKEN` or any GitHub-token env var name.

## Verdict

**Pre-fix: PASS (the secret is readable) — the finding.** Any `claude -p` subprocess —
including one whose tool-use runs an attacker-authored build/test script from a repo
under evaluation (compounding with Phase 0 before that fix) — inherited lopi's entire
process environment minus a 9+2-entry Anthropic-specific blocklist, which never covered
lopi's own operational secrets.

**Post-fix: FAIL (the secret is not readable).** `apply_env_allowlist` (called before
any other `.env()` call, at all five spawn sites — order matters: `Command::env_clear`
clears variables set via `.env()` before it runs too, not only inherited ones) replaces
inherit-all-minus-blocklist with `env_clear()` + an explicit allowlist: `PATH`, `HOME`,
`TERM`, `LANG`, `LANGUAGE`, `LC_ALL`, `LC_CTYPE`, `SHELL`, `TMPDIR`, `USER`, `LOGNAME`.
Deliberately excludes any Anthropic credential variable — the CLI locates its own
on-disk credentials at `~/.claude/` via `HOME` and has never needed an inherited API key
on any production path.

Enumeration test (`apply_env_allowlist_sets_only_the_allowlisted_vars_present_in_process_env`)
additionally proves the allowlist can never widen silently: every key
`apply_env_allowlist` sets is asserted to come from `CHILD_ENV_ALLOWLIST` itself, so
adding a future variable to that constant is a deliberate, reviewable diff.

**Named gap:** the original brief mentioned passing through "the CLI's own credential
variable" as part of the allowlist. Current code proves none is required — deliberately
omitted rather than half-implemented, since adding one would reintroduce exactly the
risk `scrub_inherited_anthropic_env` (and KT-4.1, `.konjo/killtests/F4/`) already exists
to prevent. If a future CLI version genuinely needs an inherited credential variable,
that's a new, explicit decision — not a default this sprint made silently.
