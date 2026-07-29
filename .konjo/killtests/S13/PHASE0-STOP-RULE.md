# Sprint S13, Phase 0 — Honesty pass and stop-rule verdict

**Sprint:** S13 (confirmed correct — S12 is the last recorded S-series sprint in
`.konjo/killtests/`, no renumbering needed) · **Phase:** 0 · **Verdict: STOP**

The brief's stop rule: *"If Phase 0 finds more than three self-claims with no
enforcing step, stop after Phase 0 and report."* This audit found **5**
(2 dark rubrics + 3 hard-rule bullets with no genuine enforcing step), which
is `> 3`. Per the brief, this sprint stops here. Phases 1–4, the pre-flight
kill-tests (KT-S13.1/KT-S13.2 — both scoped to gates this sprint would
introduce in Phases 1–4), and most of the post-flight deliverables did not
run. What Phase 0 itself requires (audit, correct-or-delete, corrected
baseline) is complete.

---

## 1. Corrected baseline evidence

Re-ran every command in the brief's Baseline evidence table against this
branch (`b93e68f`, `origin/main` HEAD at sprint start — 0 ahead/behind, clean
starting point).

| Claim | Original | Re-verified | Verdict |
|---|---|---|---|
| `unwrap`/`expect` outside tests | 50 sites / 6 files, all in `tests_extended.rs` + `event_bridge_bench.rs` | **60 sites / 4 files**: `crates/lopi-ui/src/web/tests_extended.rs` (50), `crates/lopi-ui/src/web/event_bridge_bench.rs` (6), `src/test_support.rs` (3), `crates/lopi-context/benches/eviction.rs` (1). Methodology: module-graph-aware scan — strip every `#[cfg(test)] mod { … }` block (string/comment-aware brace matching, not naive regex) from every non-`/tests/`, non-`*_tests.rs`/`tests.rs` file, then count remaining `.unwrap()`/`.expect(`. | **Directional match, minor drift.** Same two named files dominate (56/60); the other two (`test_support.rs`, `eviction.rs`) are the same class of file (test-helper / bench code not excluded by the `_tests.rs`/`tests/` filename filter) and were plausibly always present but not individually named in the original "6". No new production violation. |
| Production `unsafe` blocks | 0 in production; 5 test-only in `lopi-ui/src/client/auth.rs`, all `SAFETY`-commented | **Confirmed exactly**: 5 `unsafe {` blocks, all inside `#[cfg(test)] mod tests` in `crates/lopi-ui/src/client/auth.rs`, each preceded by a `// SAFETY:` comment. No `unsafe` anywhere outside that one test module. | **Matches.** |
| Raw index sites (`[0]`/`[1]`) in production | 202 | **202** (`grep -rn '\[0\]\|\[1\]'` minus `/tests/`) | **Matches exactly.** |
| `tokio::select!` sites | 4 | **4** — `lopi-ui/src/web/event_bridge.rs`, `lopi-agent/src/runner/plan_gate.rs`, `lopi-agent/src/claude_stream.rs`, `src/gap_fill_commands.rs` | **Matches exactly**, same files the brief names in Phase 2. |
| Unbounded channels | 1 production (`lopi-agent/src/quota_kill_log.rs:151`) | **2 production**, not 1: `lopi-agent/src/quota_kill_log.rs:151` **and** `src/repl/mod.rs:76` (`repl_loop`'s `mpsc::unbounded_channel::<ReplEvent>()` — not inside any `#[cfg(test)]` block, genuine production code). Three more sites in `src/repl/actions.rs` are all inside `#[cfg(test)] mod tests` and correctly excluded. | **Drift — the baseline undercounted by one.** Phase 2's item 4 ("Convert the unbounded channel at `quota_kill_log.rs:151`") would need to also cover `src/repl/mod.rs:76` to be complete; not done in this sprint since Phase 2 didn't run. |
| `std::sync::Mutex` in async crates | 1 production (`lopi-agent/src/quota_kill_log.rs:188`) | **1 production** (`quota_kill_log.rs:188`, plus its `:211` constructor — same field, not a second site). The other two hits (`lopi-ui/src/web/repos_handlers_tests.rs:13`, `lopi-ui/src/client/auth.rs:21`) are both test-only. | **Matches.** |
| Error strategy split (`thiserror` vs `anyhow`) | 14 vs 106 files | **14 vs 131 files** (non-test) | **Drift — anyhow usage grew by 25 files** since the baseline was recorded (unsurprising given ~450 files of history landed on `main` since; `thiserror` count is stable). Phase 3's crate-migration priority (`lopi-core`, `lopi-git`, `lopi-memory` first) is unaffected by this drift. |
| MSRV / toolchain pin | absent | **absent** (`rust-toolchain*` — no file; `rust-version` — no match in `Cargo.toml`) | **Matches.** |
| `[workspace.lints]` | absent | **absent** | **Matches.** |
| `overflow-checks` | absent | **absent** | **Matches.** |
| Crates / production files / LOC | 18 / 297 / ~88.6k | **18 crates** / **295 files** (non-test, non-`/tests/` naming) or **390 files** total including tests / **~88.6k LOC** (including test files) | **Crate count and LOC match; file count is sensitive to which filter is used ("production files" wasn't defined precisely enough to reproduce bit-for-bit) — not a material discrepancy.** |

---

## 2. Rubric consumer audit (brief Phase 0, item 2)

`.konjo/rubrics/*.toml` before this sprint: `feature_completeness.toml`,
`refactor_safety.toml`, `security_audit.toml`. `.konjo/scripts/konjo_review.py`
and every other file in `.konjo/scripts/` contain **zero** references to the
string `rubric` — the actual rubric loader lives in Rust
(`crates/lopi-agent/src/verifier.rs`), not the Python CI tooling the brief's
phrasing implied.

| Rubric file | Real consumer | Decision |
|---|---|---|
| `feature_completeness.toml` | **Yes.** `verifier::resolve_rubric` (`crates/lopi-agent/src/verifier.rs:127-134`) hardcodes `DEFAULT_RUBRIC_FILE = "feature_completeness"` and loads it via `load_rubric_file` at `verifier.rs:131`; wired to production through `crates/lopi-agent/src/runner/verifier_runner.rs:34`. | **Keep.** |
| `refactor_safety.toml` | **No consumer.** `load_rubric_file(repo, name)` is generic and *could* load it, but no call site anywhere passes `"refactor_safety"` — only `KONJO_VERIFIER.md`'s doc example does. | **Deleted** (`.konjo/rubrics/refactor_safety.toml`). Doc claims in `KONJO_VERIFIER.md` and `PLAN.md` corrected in the same commit. Ledger entry added (one-way door: any future rubric needs both the file *and* a real call site before being documented as "shipping"). |
| `security_audit.toml` | **No consumer.** Same as above — never loaded by name anywhere in the codebase. | **Deleted.** Same doc corrections. |

## 3. `CLAUDE.md` "Additional Hard Rules" audit (brief Phase 0, item 3)

Read the full `.github/workflows/konjo-gate.yml` (790 lines) and every script
under `.konjo/scripts/` to find the exact enforcing job:step for each bullet.

| # | Bullet | Enforcing job:step | Verdict |
|---|---|---|---|
| 1 | Coverage ≥ 80% hard block; 95% target | `coverage:"Coverage gate (80% floor, 95% target)"` exists but is `continue-on-error: true` (`konjo-gate.yml:307-347`, flag at :324) — cannot fail the build. The **real** hard gate is a different, lower bar: `coverage:"Coverage floor gate (never regress below the locked value)"` against `.konjo/coverage-floor.txt` (`konjo-gate.yml:349-362`). | **No enforcing step for the 80%/95% claim as stated.** Bullet corrected in `CLAUDE.md` to describe the real hard gate (the locked floor) and demote 80%/95% to labeled soft targets. |
| 2 | Zero cognitive complexity > 15 | `complexity:"Cognitive complexity gate (clippy)"`, no `continue-on-error`, `exit 1` on violation (`konjo-gate.yml:467-494`) | Hard, genuine. Kept as-is. |
| 3 | Zero dead code | `static:"dead code — zero tolerance"`, `exit 1` if count > 0 (`konjo-gate.yml:138-147`) | Hard, genuine. Kept as-is. |
| 4 | Zero undocumented public APIs | `complexity:"Documentation gate (rustdoc)"` runs the exact `-D missing_docs` command but is `continue-on-error: true` (`konjo-gate.yml:541-564`, flag at :557; comment cites known doc-link debt in `lopi-agent`/`lopi-orchestrator` as of 2026-07). | **No enforcing step — soft only.** Bullet corrected to say so explicitly. |
| 5 | Function body ≤ 50 lines (30 target) | **None found anywhere** in `konjo-gate.yml` or `.konjo/scripts/`. Only appears as Q7 in the Wall-3 LLM review prompt (`konjo_review.py:84-87`), and Q7 is explicitly `WARNING` tier in that script's own verdict rules (`konjo_review.py:105-109`), so even a real violation the LLM catches cannot fail `review:"Fail if BLOCKER verdict"`. | **No enforcing step at all.** Bullet corrected to say it's not mechanically enforced. |
| 6 | File ≤ 500 lines (300 target) | `complexity:"File size gate (changed files on PR, all files on push)"`, hard, `exit 1` (`konjo-gate.yml:496-527`) — but scoped to `*.rs`/`*.py` only; `web/` (TS/Svelte) and `macos/` (Swift) are not covered. | Hard for its stated scope; bullet annotated with the scope caveat. |
| 7 | No duplicate blocks > 10 lines at > 85% similarity | `complexity:"DRY check"` invokes `dry_check.py` directly, hard, `exit 1` (`konjo-gate.yml:529-539`) — but CI passes `--min-lines 20` (`:531-533`), not 10; the script's own default is 10. | Hard and genuine, but the threshold in the bullet was wrong (20, not 10). Corrected. |
| 8 | `cargo audit` / `cargo deny` zero violations | `static:"cargo audit — zero known vulnerabilities"` and `static:"cargo deny — license + advisory + bans"`, both hard, no `continue-on-error` (`konjo-gate.yml:104-122`, `:124-136`) | Hard, genuine. Kept as-is. |

**3 of 8 bullets (#1, #4, #5) had no genuine enforcing step** — corrected in
`CLAUDE.md` in this commit rather than newly wired (wiring dark gates back on
is Wave 1 work, tracked separately per the brief's non-goals; Phase 0's job is
an honest inventory, not a fix).

## 4. `.claude/rules/*.md` path-glob audit (brief Phase 0, item 4)

| Rule file | Dead glob(s) found | Fix applied |
|---|---|---|
| `benchmarking.md` | `**/bench_*.rs` (0 matches — real files are `*_bench.rs`, e.g. `event_bridge_bench.rs`); `**/perf/**` (0 matches — no `perf/` directory exists anywhere in the repo) | Replaced `bench_*.rs` → `*_bench.rs`; replaced `perf/**` → `benches/**` (matches `crates/lopi-toon/benches/`, `crates/lopi-context/benches/`). Verified both new globs match real files. |
| `testing.md` | `**/*_test.rs` (0 matches — real convention is plural `_tests.rs`); `**/spec/**` (0 matches — no literal `spec/` dir; the closest concept is the `lopi-spec` crate) | Replaced `*_test.rs` → `*_tests.rs`; replaced `spec/**` → `lopi-spec/**`. Verified both new globs match real files. |
| `security.md` | none — all 6 patterns matched | No change. |
| `rust-conventions.md` | none — `**/*.rs` trivially matches | No change. |
| `git-workflow.md` | N/A — no frontmatter/`paths:` key, loads unconditionally (an intentional "always apply" rule, not a bug) | No change. |

These two files' broken globs were a **rule that never loads under its
stated trigger condition** since the day they were written — the same failure
class as an unenforced hard rule, just at the always-loaded-context layer
instead of CI. Not counted toward the stop-rule's "self-claims with no
enforcing step" tally above (that tally is scoped to the brief's item 2/3:
rubrics and `CLAUDE.md` hard rules), but flagged here as the same defect.

---

## 5. Stop-rule tally and what did not run

| Source | Unmapped claims |
|---|---|
| Dark rubrics (item 2) | 2 (`refactor_safety.toml`, `security_audit.toml`) |
| Unenforced `CLAUDE.md` hard-rule bullets (item 3) | 3 (#1 coverage, #4 doc coverage, #5 function length) |
| **Total** | **5 > 3 → stop rule triggers** |

Per the brief: *"A repo that misdescribes its own gates should not have more
gates added to it until the description is true."* Accordingly, **not
performed in this sprint**:

- Pre-flight kill-tests KT-S13.1 (fixture pairs for gates this sprint would
  introduce) and KT-S13.2 (`overflow-checks` reaching every workspace crate)
  — both are scoped to gates/changes Phase 1 introduces, which did not run.
- Phase 1 (determinism substrate: `rust-toolchain.toml`, MSRV bisection,
  `[workspace.lints]`, `overflow-checks`).
- Phase 2 (indexing floor ratchet, checked/saturating arithmetic,
  `CANCEL-SAFETY:` comments, bounded channel for `quota_kill_log.rs`).
- Phase 3 (error taxonomy for `lopi-core`/`lopi-git`/`lopi-memory`).
- Phase 4 (`CLAUDE.md` security/resource constraints, `security.md` split,
  threat block in `konjo-boot`, `SessionStart` hook, hardcoded-path fix in
  `.claude/settings.json`, `post-edit.sh` extension, CI wiring for new gates).

**What did complete, as Phase 0's own scope:** the corrected baseline table
above; deletion of the 2 dark rubrics and correction of every doc that
claimed 3 rubrics ship (`KONJO_VERIFIER.md`, `PLAN.md`); correction of the 3
false-hard `CLAUDE.md` bullets (and the 2 threshold errors found alongside
them, #6's scope caveat and #7's actual 20-line CI threshold); repair of the
4 dead rule-file globs. This document is the audit trail; `LEDGER.md` and
`CHANGELOG.md` carry the one-way-door and dated-entry records.

## Next session

Re-run this same audit (or a scoped subset) before resuming Phase 1 — the
sprint should not proceed past Phase 0 until a re-run finds ≤ 3 unmapped
claims, per the brief's own rule. See `NEXT_SESSION_PROMPT.md`.
