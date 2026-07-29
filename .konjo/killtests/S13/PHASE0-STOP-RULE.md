# Sprint S13, Phase 0 — Honesty pass and stop-rule verdict

**Sprint:** S13 (confirmed correct — S12 is the last recorded S-series sprint in
`.konjo/killtests/`, no renumbering needed) · **Phase:** 0 · **Verdict: STOP**

The brief's stop rule: *"If Phase 0 finds more than three self-claims with no
enforcing step, stop after Phase 0 and report."* This audit — run independently
against a clean checkout at `origin/main` HEAD (`b93e68f`) — found **5**
unmapped claims (2 dark rubrics + 3 hard-rule bullets with no genuine
enforcing step), which is `> 3`. Per the brief, this sprint stops here.
Phases 1–4, the pre-flight kill-tests (KT-S13.1/KT-S13.2 — both scoped to
gates this sprint would introduce in Phases 1–4), and most of the post-flight
deliverables did not run. What Phase 0 itself requires (audit, correct-or-
delete, corrected baseline) is complete.

This is a second, independent run of the same audit against the same base
commit — a prior session already produced this verdict on a sibling branch
(`claude/s13-quality-substrate-e7e92y`, open as draft PR #182). The methodology
and numbers below were derived from scratch, not copied from that PR, and are
compared against it at the end of each section as a cross-check.

---

## 1. Baseline evidence, re-verified from a clean checkout

Base commit: `b93e68f` (`origin/main` HEAD; unmoved since PR #182 was opened).

| Claim | Brief's original | Independently re-verified | Verdict |
|---|---|---|---|
| `unwrap`/`expect` outside tests | 50 sites / 6 files, all in `tests_extended.rs` + `event_bridge_bench.rs` | **60 sites / 4 files**: `crates/lopi-ui/src/web/tests_extended.rs` (50), `crates/lopi-ui/src/web/event_bridge_bench.rs` (6), `src/test_support.rs` (3), `crates/lopi-context/benches/eviction.rs` (1). Method: a brace/string/comment-aware Python script strips every `#[cfg(test)] mod { … }` block (handling Rust raw strings `r#"..."#`, which a naive parser mis-scans) from every file not under a `tests/`/`benches/` directory or matching a test-ish filename, then counts remaining `.unwrap()`/`.expect(`. Zero remaining in genuinely-production files once `benches/` dirs and test-named files are excluded. | **Matches PR #182's re-verified number (60/4) exactly.** Confirms the brief's original count undercounted by not naming `test_support.rs`/`eviction.rs`. |
| Production `unsafe` blocks | 0 in production; 5 test-only in `lopi-ui/src/client/auth.rs`, all `SAFETY`-commented | **Confirmed exactly**: `grep -rn "unsafe "` finds 8 hits total, 3 of which are the word "unsafe" inside doc comments (not code); the remaining 5 are `unsafe {` blocks, all inside `#[cfg(test)] mod tests` in `crates/lopi-ui/src/client/auth.rs`, each preceded by a `// SAFETY:` comment. | **Matches brief and PR #182.** |
| Raw index sites (`[0]`/`[1]`) in production | 202 | **185** using a documented methodology (exclude `/tests/` and `/benches/` directories, and filenames matching a test/bench naming pattern). Excluding only `/tests/` directories (no filename filter) gives 341; excluding directories + filenames but keeping benches gives 194. The count is highly sensitive to exactly which files count as "test" — the brief's own phrasing ("minus test files") doesn't pin this down precisely enough to reproduce bit-for-bit. | **Drift, both from the brief (202) and from PR #182's independently re-verified 202** — despite three different concrete filter definitions tried here, none reproduced 202 exactly. This is the one number in this table that could not be independently reconciled; recorded as an open discrepancy rather than papered over. Does not change the stop-rule tally (this claim isn't part of that count) but is relevant context for Phase 2's indexing-floor seed value, should the sprint resume — whoever seeds the floor must fix one precise methodology and document it in the ratchet script itself, not carry this ambiguity forward. |
| `tokio::select!` sites | 4 | **4** — `crates/lopi-ui/src/web/event_bridge.rs`, `crates/lopi-agent/src/runner/plan_gate.rs`, `crates/lopi-agent/src/claude_stream.rs`, `src/gap_fill_commands.rs` | **Matches brief and PR #182 exactly**, same four files. |
| Unbounded channels | 1 production (`lopi-agent/src/quota_kill_log.rs:151`) | **2 production**: `lopi-agent/src/quota_kill_log.rs:151` **and** `src/repl/mod.rs:76` (`repl_loop`'s `mpsc::unbounded_channel::<ReplEvent>()`, confirmed not inside any `#[cfg(test)]` block by scanning every line above it in the file for `cfg(test)`). Three more `unbounded_channel` sites in `src/repl/actions.rs` (lines 266/277/290) are confirmed inside `#[cfg(test)] mod tests` (starts line 247) and correctly excluded. | **Matches PR #182's re-verified number (2) exactly** — the brief's original "1" undercounts by missing `repl/mod.rs:76`. |
| `std::sync::Mutex` in async crates | 1 production (`lopi-agent/src/quota_kill_log.rs:188`) | **1 production** (`quota_kill_log.rs:188`, field; `:211` is that field's constructor, same site). `lopi-ui/src/web/repos_handlers_tests.rs:13` (filename-excluded, test file) and `lopi-ui/src/client/auth.rs:21` (confirmed inside `#[cfg(test)] mod tests`, starts line 17) are both test-only. | **Matches brief and PR #182.** |
| Error strategy split (`thiserror` vs `anyhow`) | 14 vs 106 files | **14 vs 131** (non-test `.rs` files; 132 including one test file). | **Matches PR #182's re-verified number (131) almost exactly** (132 vs 131 all-files count — one-file rounding difference, immaterial). Confirms `anyhow` usage grew substantially since the brief's baseline was recorded; `thiserror` count is stable. |
| MSRV / toolchain pin | absent | **absent** — no `rust-toolchain*` file, no `rust-version` key in `Cargo.toml`. | **Matches.** |
| `[workspace.lints]` | absent | **absent.** | **Matches.** |
| `overflow-checks` | absent | **absent.** | **Matches.** |
| Crates / production files / LOC | 18 / 297 / ~88.6k | **18 crates**, **390 total `.rs` files** (307 by a non-test-filename filter, close to but not identical to the brief's 297 — "production files" isn't defined precisely enough to reproduce exactly, same caveat PR #182 raised), **~88.6k LOC total**. | **Crate count and LOC match exactly; file count is the same soft-methodology mismatch PR #182 already flagged, not a new discrepancy.** |

---

## 2. Rubric consumer audit (brief Phase 0, item 2)

`.konjo/rubrics/*.toml` before this sprint: `feature_completeness.toml`,
`refactor_safety.toml`, `security_audit.toml`. Independently confirmed:
`grep -rli "rubric" .konjo/scripts/` returns **zero** files — the rubric
loader is Rust code (`crates/lopi-agent/src/verifier.rs`), not the Python CI
tooling.

| Rubric file | Real consumer | Decision |
|---|---|---|
| `feature_completeness.toml` | **Live.** `verifier::resolve_rubric` (`crates/lopi-agent/src/verifier.rs:127-134`) hardcodes `DEFAULT_RUBRIC_FILE = "feature_completeness"`, loaded via `load_rubric_file` at `verifier.rs:131`, wired through `crates/lopi-agent/src/runner/verifier_runner.rs:34`. | **Keep.** |
| `refactor_safety.toml` | **No consumer.** `load_rubric_file(repo, name)` is generic and *could* load any name, but no call site anywhere passes `"refactor_safety"` — grepped every `.rs` file under `crates/`/`src/`; the only hits are `KONJO_VERIFIER.md`'s doc example (`load_rubric_file(repo, "refactor_safety")`) and an unrelated inline TOML fixture string in `crates/lopi-core/src/task_tests.rs` that happens to name a rubric `"refactor_safety"` for a parser unit test — not a file load. | **Deleted** (`.konjo/rubrics/refactor_safety.toml`). `KONJO_VERIFIER.md` and `PLAN.md` corrected in this commit; `LEDGER.md` carries the one-way-door (a future rubric needs both the file and a real call site before being documented as shipping). |
| `security_audit.toml` | **No consumer.** Same grep, same result — never loaded by name anywhere in the codebase; only appears in `KONJO_VERIFIER.md` (as an inline `Rubric` struct literal example, not a file load — left as-is) and `PLAN.md`/`CHANGELOG.md`. | **Deleted.** Same doc corrections as above. |

**Cross-check against PR #182:** identical verdict on all three files.

## 3. `CLAUDE.md` "Additional Hard Rules" audit (brief Phase 0, item 3)

Read the full 789-line `.github/workflows/konjo-gate.yml` end to end and
`.konjo/scripts/konjo_review.py` to find the exact enforcing job:step for
each of the 8 bullets.

| # | Bullet | Enforcing job:step | Verdict |
|---|---|---|---|
| 1 | Coverage ≥ 80% hard block; 95% target | `coverage:"Coverage gate (80% floor, 95% target)"` exists (`konjo-gate.yml:307-347`) but carries `continue-on-error: true` at line 324 — cannot fail the build regardless of measured coverage. The gate that **can** fail the build is a different, lower bar: `coverage:"Coverage floor gate (never regress below the locked value)"` (`:349-362`), which compares against `.konjo/coverage-floor.txt`, not 80%. | **No enforcing step for the 80%/95% claim as stated.** Corrected in `CLAUDE.md`. |
| 2 | Zero cognitive complexity > 15 | `complexity:"Cognitive complexity gate (clippy)"`, `:467-494`, no `continue-on-error`, `exit 1` on any violation. | Hard, genuine. Unchanged. |
| 3 | Zero dead code | `static:"dead code — zero tolerance"`, `:138-147`, `exit 1` if count > 0. | Hard, genuine. Unchanged. |
| 4 | Zero undocumented public APIs | `complexity:"Documentation gate (rustdoc)"`, `:541-564`, runs the exact `-D missing_docs -D rustdoc::broken_intra_doc_links` command stated but carries `continue-on-error: true` at `:557` — the step's own comment cites known broken-intra-doc-link debt in `lopi-agent`/`lopi-orchestrator` as of 2026-07. | **No enforcing step — soft only.** Corrected. |
| 5 | Function body ≤ 50 lines (30 target) | **None found anywhere** in `konjo-gate.yml` or `.konjo/scripts/*.py` (grepped for `50 line`, `function_length`, `fn_length`, `body.*50`). Only appears as Q7 in the Wall-3 LLM review prompt (`konjo_review.py:84-86`), and the script's own verdict rules put Q7 at WARNING tier explicitly (`konjo_review.py:108`: *"Issue WARNING for: Q2 partial coverage, Q6 minor duplication, Q7 complexity"*) — only a BLOCKER verdict fails `review:"Fail if BLOCKER verdict"` (`:722-735`), so a real Q7 violation the LLM catches still can't block merge. | **No enforcing step at all.** Corrected. |
| 6 | File ≤ 500 lines (300 target) | `complexity:"File size gate (changed files on PR, all files on push)"`, `:496-527`, hard, `exit 1` — but the step's own name/comment states it is scoped to `*.rs`/`*.py` only (`grep -E '\.(rs|py)$'` at `:510`/`:512`); `web/` (TS/Svelte) and `macos/` (Swift) are uncovered. | Hard for its stated scope; annotated with the scope caveat. |
| 7 | No duplicate blocks > 10 lines at > 85% similarity | `complexity:"DRY check"`, `:529-539`, hard, `exit 1`, invokes `dry_check.py --threshold 0.85 --min-lines 20` — CI's actual line-count threshold is **20**, not the 10 the bullet states. | Hard and genuine, but the bullet's stated threshold was wrong. Corrected to 20. |
| 8 | `cargo audit` / `cargo deny` zero violations | `static:"cargo audit — zero known vulnerabilities"` (`:104-122`) and `static:"cargo deny — license + advisory + bans"` (`:124-136`), both hard, no `continue-on-error`. | Hard, genuine. Unchanged. |

**3 of 8 bullets (#1, #4, #5) have no genuine enforcing step** — corrected in
`CLAUDE.md` in this commit (wiring the dark gates back on is Wave 1 work,
tracked separately, per the brief's non-goals). Two more (#6, #7) are
genuinely hard but were stated with an inaccurate scope/threshold, corrected
alongside.

**Cross-check against PR #182:** identical set of 3 unenforced bullets
(#1, #4, #5) and identical two threshold/scope corrections (#6, #7).

## 4. `.claude/rules/*.md` path-glob audit (brief Phase 0, item 4)

Every glob checked against the real file tree with `find`.

| Rule file | Dead glob(s) found | Fix applied |
|---|---|---|
| `benchmarking.md` | `**/bench_*.rs` — 0 matches (real files use the suffix form `*_bench.rs`, e.g. `crates/lopi-ui/src/web/event_bridge_bench.rs`). `**/perf/**` — 0 matches, no `perf/` directory exists anywhere in the repo. | `bench_*.rs` → `*_bench.rs` (verified: matches `event_bridge_bench.rs`); `perf/**` → `benches/**` (verified: matches `crates/lopi-toon/benches/`, `crates/lopi-context/benches/`). |
| `testing.md` | `**/*_test.rs` — 0 matches (real convention is plural `_tests.rs`; 64 files match that plural form). `**/spec/**` — 0 matches, no literal `spec/` directory (closest concept is the `lopi-spec` crate). | `*_test.rs` → `*_tests.rs` (verified: 64 matches); `spec/**` → `lopi-spec/**` (verified: matches `crates/lopi-spec/`). |
| `security.md` | None — all 7 patterns (`lopi-ui/**`, `lopi-webhook/**`, `lopi-remote/**`, `api*`, `server*`, `webhook*`, `auth*`) matched real files/dirs. | No change. |
| `rust-conventions.md` | None — `**/*.rs` trivially matches all 390 Rust files. | No change. |
| `git-workflow.md` | N/A — no frontmatter/`paths:` key at all; loads unconditionally (an intentional always-apply rule, not a bug). | No change. |

Not counted toward the stop-rule tally (scoped to the brief's item 2/3:
rubrics and `CLAUDE.md` bullets) but flagged as the same defect class: a rule
that never loads under its stated trigger condition, at the always-loaded-
context layer instead of CI.

**Cross-check against PR #182:** identical two dead-glob files and identical
replacement globs.

---

## 5. Stop-rule tally and what did not run

| Source | Unmapped claims |
|---|---|
| Dark rubrics (item 2) | 2 (`refactor_safety.toml`, `security_audit.toml`) |
| Unenforced `CLAUDE.md` hard-rule bullets (item 3) | 3 (#1 coverage, #4 doc coverage, #5 function length) |
| **Total** | **5 > 3 → stop rule triggers** |

This is the identical tally PR #182 reached, arrived at independently.

Per the brief: *"A repo that misdescribes its own gates should not have more
gates added to it until the description is true."* Accordingly, **not
performed in this sprint**:

- Pre-flight kill-tests KT-S13.1 (fixture pairs for gates this sprint would
  introduce) and KT-S13.2 (`overflow-checks` reaching every workspace crate)
  — both scoped to gates/changes Phase 1 introduces, which did not run.
- Phase 1 (determinism substrate: `rust-toolchain.toml`, MSRV bisection,
  `[workspace.lints]`, `overflow-checks`).
- Phase 2 (indexing floor ratchet, checked/saturating arithmetic,
  `CANCEL-SAFETY:` comments, bounded channel for `quota_kill_log.rs`).
- Phase 3 (error taxonomy for `lopi-core`/`lopi-git`/`lopi-memory`).
- Phase 4 (`CLAUDE.md` security/resource constraints, `security.md` split,
  threat block in `konjo-boot`, `SessionStart` hook, hardcoded-path fix in
  `.claude/settings.json`, `post-edit.sh` extension, CI wiring for new gates).

**What did complete, as Phase 0's own scope:** the re-verified baseline table
above; deletion of the 2 dark rubrics and correction of every doc claiming 3
rubrics ship (`KONJO_VERIFIER.md`, `PLAN.md`); correction of the 3 false-hard
`CLAUDE.md` bullets (plus the 2 threshold/scope corrections found alongside
them, #6's scope caveat and #7's actual 20-line CI threshold); repair of the
4 dead rule-file globs. This document is the audit trail; `LEDGER.md` and
`CHANGELOG.md` carry the one-way-door and dated-entry records.

## 6. Relationship to PR #182

`claude/s13-quality-substrate-e7e92y` (PR #182, open draft) ran this exact
audit against the identical base commit and reached the identical stop-rule
verdict, dark-rubric decisions, hard-rule corrections, and glob fixes. This
document is an independently-derived second pass — different scripts,
different intermediate methodology, same conclusions on every claim except
the raw-indexing count (§1), which neither run could pin down to an exact,
reproducible number under the brief's own loosely-specified filter. Given two
independent audits agree on the stop-rule-determining tally, a third
re-audit is not expected to change the verdict; **resuming past Phase 0
should start from re-running §1's baseline table (numbers may have moved)
and confirming the tally in §5 is still ≤ 3 before touching Phase 1**, not
from repeating the full audit a third time.

## Next session

See `NEXT_SESSION_PROMPT.md`.
