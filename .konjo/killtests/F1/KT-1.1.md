# KT-1.1 — Structured verdicts via `--json-schema`

**Verdict: PASS.** Design Phase 1 around `--json-schema`.

## Method

Ran, against a real Claude subscription (no `ANTHROPIC_API_KEY` in the
environment) — see KT-1.3 for why `--bare` is absent:

```bash
claude -p "<verifier prompt>" \
  --output-format json \
  --json-schema '{"type":"object","properties":{"passed":{"type":"boolean"},"gaps":{"type":"array","items":{"type":"string"}},"fix_hints":{"type":"array","items":{"type":"string"}},"confidence":{"type":"number"}},"required":["passed","gaps","fix_hints","confidence"],"additionalProperties":false}' \
  --disallowedTools "Write,Edit,MultiEdit,NotebookEdit,Bash"
```

30 times, against a fixed prompt grading a small, clean diff (add a doc
comment to a two-line function) against the repo's default rubric.

**Sub-finding, discovered before the 30-run loop:** `--json-schema` takes the
schema **inline as a JSON string**, not a file path — `--json-schema
./verdict.schema.json` fails with `JSON Parse error: Unrecognized token '/'`.
The brief's own example command passes a path; that form does not work
against this CLI version (2.1.220). Corrected before running the 30x loop.

**Second sub-finding:** `-p <prompt>` must be placed **before** any
`<tools...>`-variadic flag (`--disallowedTools`, `--allowedTools`). Commander
greedily consumes trailing non-flag tokens into a variadic option, so a
prompt placed after `--disallowedTools "A,B,C"` gets silently swallowed as an
extra tool name and the CLI exits with "Input must be provided either
through stdin or as a prompt argument."

## Result

30/30 lines: `is_error: false`, `structured_output` present and matching the
schema exactly (`passed`/`gaps`/`fix_hints`/`confidence`, correct types, no
extra keys). Checked programmatically against the schema's required-key set.
The existing fence-strip fallback parser (`verifier.rs::strip_fences` +
`serde_json::from_str`) also succeeded 30/30 against the same responses'
`result` field — the malformed rate the brief asks to record for the
fallback path is **0/30** in this run, so it costs nothing to keep as a
defense-in-depth fallback behind the primary `structured_output` parse.

Raw output: `/tmp/.../scratchpad/kt1/kt11_results.jsonl` (ephemeral scratch
path, not committed — 30 raw CLI JSON envelopes).

## Design consequence

Phase 1's CLI backend parses `structured_output` first; on absence or a
schema mismatch it falls back to the existing `strip_fences` +
`serde_json::from_str` parser against the `result` field — the same parser
already used by the API backend, so no new parsing logic exists for the
fallback path, only a new field to check first.
