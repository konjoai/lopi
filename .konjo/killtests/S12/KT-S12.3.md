# KT-S12.3 — task-scope confinement inventory (not pass/fail)

**Sprint:** S12, Phase 3 · **Type:** inventory, per the sprint brief's own framing — "not
pass/fail." Full table with `file:line` citations and verdicts lives in
`docs/security/TRIFECTA_PATHS.md` §7 ("Sprint S12, Phase 3 — task-scope confinement").

## Summary of what §7 records

| # | Question | Verdict |
|---|---|---|
| 1 | Repo confinement against the operator's configured repo list | **Unenforced** — no allowlist exists to check against |
| 2 | `allowed_dirs`/`forbidden_dirs` structural vs. advisory | **Mixed** — prompt/plan-review layer is advisory-only; `DiffChecker` is real, post-hoc enforcement |
| 3 | Untrusted-source task reaching an unauthorized repo | **Unenforced** — same root cause as row 1, provenance-blind in both directions |
| 4 | Worktree escape via symlink/absolute path/`..` | **Unenforced** — confinement is `current_dir` convention only, no path validation on individual tool calls |
| 5 | `gate_untrusted_source` coverage | **Enforced** for every row already in TRIFECTA §6 (A–D, K) and for successor/chained tasks (`derive_successor_task`). **New gap:** the `lopi_submit_task` MCP tool never checks source trust at all |

Two additional items recorded in §7: a deliberate decision not to patch the MCP gap this
sprint (needs session-provenance plumbing lopi doesn't have, not a policy engine — see §7's
"Why row 5's MCP gap is named, not patched, this sprint"), and a documentation-drift fix
(`crates/lopi-core/src/config.rs`'s `bypass_permissions` doc comment, which implied real
enforcement it never provided — corrected this sprint, `src/repl/state.rs:67` confirmed as its
only, display-only consumer).

## Why this is a kill-test file and not just a doc

Per the sprint brief: "Fix only what the inventory shows is reachable. Do not build a policy
engine for a threat model that ends at one machine." Four of five rows above are unenforced or
mixed — named honestly rather than smoothed over — and none were patched this sprint, because
each would require either a real design decision (row 1/3: what should "the operator's
configured repo list" mean when `lopi.toml` doesn't currently model one) or infrastructure this
sprint didn't build (row 4: sandboxing tool-call paths; row 5: session-provenance propagation
through the MCP transport). Recording them here, with citations, is the deliverable — silently
closing this file with "no bugs found" would misrepresent what was actually checked.
