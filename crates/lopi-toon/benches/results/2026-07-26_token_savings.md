# TOON token-savings measurement — 2026-07-26

**Method:** for each payload shape, build one `serde_json::Value` per corpus sample and compare `serde_json::to_string` (compact JSON) against `lopi_toon::encode` on the identical value. Token counts are cumulative across the shape's full corpus, not per-sample averages.
**Tokenizer:** `tiktoken-rs` `cl100k_base` — OpenAI's GPT-4 BPE, **not** a Claude token count. No `ANTHROPIC_API_KEY` was available when this was run; see `crates/lopi-toon/src/lib.rs` and the README for this caveat.
**Corpus:** 37 real task goals (27 from `artifacts/diagnostics/20260717T113652Z/tasks.json`, 10 from `benchmarks/run.sh` T01-T10) × 2 real `allowed_dirs`/`forbidden_dirs` sets (shipped `lopi.toml.example` defaults; a crate-scoped set) = 74 samples for dir-only shapes. The full-context shape additionally attaches this repo's own 5 `CLAUDE.md` constraints and representative (schema-conformant, not live-table) pattern/lesson rows.

| Shape | Call site | n | JSON tokens | TOON tokens | Savings |
|---|---|---|---|---|---|
| plan_streamed (full context) | claude.rs plan_streamed() | 74 | 19568 | 18774 | 4.1% |
| implement_streamed (dirs only) | claude.rs implement_streamed() | 74 | 3436 | 3308 | 3.7% |
| allowed-dirs only | claude.rs implement_step() | 74 | 2622 | 2605 | 0.6% |
| dirs + constraint array (marginal) | claude_support.rs build_plan_prompt() constraints slice | 74 | 9134 | 9154 | -0.2% |
| dirs + pattern table (marginal) | claude_support.rs build_plan_prompt() patterns slice | 74 | 8246 | 7748 | 6.0% |

**Overall (cl100k, all shapes pooled): 3.3% fewer tokens than compact JSON.**

Note: `fix()` (`crates/lopi-agent/src/claude.rs`) does not call `encode_task_context` — it hand-rolls an `allowed[N]: a,b,c` line and the doc comment there states TOON is skipped for it (error text is free-form prose). The "dirs-only" shape above measures the same `encode_task_context(goal, allowed, &[], &[], &[], &[])` call used by `implement_step()`, which is structurally what `fix()` would produce if it adopted TOON — it is not currently exercised by `fix()` itself.

## Phase 2 — per-field marginal savings (cl100k, vs. dirs-only baseline, n=74)

- Adding the constraint array (5 real `CLAUDE.md` constraints) to a dirs-only prompt: **-2.0 tokens/prompt**.
- Adding the pattern table (3 representative keyword/constraint rows) to a dirs-only prompt: **5.0 tokens/attempt**.
- These replace the unsourced `~17/prompt` and `~158/attempt` figures previously in `claude.rs:5-6` — both were far higher than what this corpus measures.
