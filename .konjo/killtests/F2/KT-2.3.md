# KT-2.3 — Does `--effort` still behave on the current generation?

**Sprint:** F2 · **Verdict:** PASS — `--effort` behaves unaffected by adaptive thinking on Sonnet 5.

## Method

Ran the exact commands the brief specified, against a real, already-authenticated
`claude` CLI subscription available in this environment (`claude` 2.1.220):

```
$ claude -p --model claude-sonnet-5 --effort high "say ok"
ok

$ claude -p --model claude-sonnet-5 --effort max "say ok"
Ok.
```

Both commands exited 0 and returned a normal completion — no 400, no error
about disabled/adaptive thinking conflicting with `--effort`, at either level.

## Verdict

**Pass.** `--effort` is a CLI-level flag (`claude_support.rs`'s
`apply_cli_caps`/`normalize_effort`), not a raw API `thinking`/sampling
parameter — the CLI resolves it internally however it resolves adaptive
thinking on Sonnet 5, and that resolution is invisible to lopi's spawn code.
`normalize_effort`'s level list (`low`/`medium`/`high`/`xhigh`/`max`,
`claude_support.rs:77-82`) stays as-is; no change needed for Phase 4.

Confirms the brief's own framing: the Sonnet 5 sampling-parameter 400 (raw API
`temperature`/`top_p`/`top_k`, and manual `thinking: {enabled, budget_tokens}`)
is a **raw Messages API** concern. lopi's `--effort` flows through the `claude`
CLI's own argument parsing, a different surface entirely — confirmed live
rather than assumed.
