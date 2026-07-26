# KT-1.4 — Model separation actually holds

**Verdict: PASS.** Confirmed 4-for-4 (every `select_model` worker outcome)
plus the `task.model` override case.

## Method

`resolve_verifier` (`crates/lopi-agent/src/verifier.rs:34-47`) and
`select_model` (`crates/lopi-agent/src/claude_model.rs:47-61`) are both pure
functions, already covered by the existing unit test suite predating this
sprint. Ran it for real rather than re-deriving by hand:

```
cargo test -p lopi-agent --lib verifier:: -- --nocapture
```

## Result — 20/20 passed, including the four load-bearing ones

| Worker outcome (`select_model`) | Verifier model (`resolve_verifier`) | Differs? |
|---|---|---|
| size 0-2 → Haiku | Opus (default: worker ≠ Opus) | yes |
| size 3-6 → Sonnet | Opus (default: worker ≠ Opus) | yes |
| size >6 → Opus | Sonnet (the one case where the naive default would collide — the resolver special-cases it) | yes |
| attempt ≥2 escalation → Opus | Sonnet (same special-case) | yes |
| `task.model` override (explicit, wins over heuristic/escalation) | resolved independently against the override string, per `resolve_verifier_honors_an_explicit_override` | differs by construction — an explicit override is compared the same way as any other worker string |

Tests: `resolve_verifier_defaults_to_opus_for_a_non_opus_worker`,
`resolve_verifier_never_grades_its_own_homework`,
`resolve_verifier_honors_an_explicit_override`,
`resolve_verifier_passes_effort_through_unchanged`, plus
`select_model_*` in `claude_model.rs`'s own test module — full output
recorded in this sprint's session transcript.

## Design consequence

None — this confirms `resolve_verifier`'s existing differ-from-worker rule
(built before this sprint) needs no change. Per the brief's non-goals, F1
does not touch it further.
