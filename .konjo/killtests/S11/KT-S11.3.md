# KT-S11.3 — `.exists()` TOCTOU inventory across `crates/` and `src/`

**Sprint:** S11 Round 2, Phase 3 · **Kind:** enumeration + targeted fix, not a
single pass/fail kill-test — the deliverable is the inventory table in §2, per
the Phase 3 brief. Table shape follows `docs/security/TRIFECTA_PATHS.md` §6.

## Method

The brief's own framing: 27 `.exists()` call sites across `crates/` and
`src/` outside tests, not individually reviewed prior to this sprint, sorted
by reachability rather than by count.

1. `grep -rn "\.exists()" crates/ src/ --include=*.rs`, excluding
   `crates/lopi-ui/src/web/*` (a parallel workstream this phase must not
   touch — see `KT-S11.3`'s parent instructions) and excluding files/dirs
   that are unambiguously test code (`/tests/` dirs, `tests.rs` files).
   This first pass landed on 27 lines — matching the brief's count.
2. Each of the 27 lines was read in its enclosing function, not just
   grepped. Four turned out to be test code a filename-based filter can't
   catch: two are helper functions (`tempdir()`-style fixtures) living
   inside a `#[cfg(test)] mod tests` block in an otherwise-production file,
   one is an assertion inside a bare `#[tokio::test]` function, one is an
   assertion inside `#[cfg(test)] mod tests` in a file whose name doesn't
   end in `test(s).rs`. These four are marked "test code" below and
   excluded from the reachability analysis — the true production surface
   is 23 lines (24 individual `.exists()` calls, since one line checks two
   filenames).
3. Each production site was classified TOCTOU-reachable, Not-reachable, or
   Benign-by-design per the brief's definitions (reachable = a concurrent
   agent/task/git operation can change the check-to-use gap's outcome AND
   that has a real consequence; benign = a stale answer either way is
   harmless by design).
4. Every TOCTOU-reachable site got a local fix: check-then-open replaced
   with open-and-classify-the-error (`ErrorKind::NotFound` → the same
   fallback the `exists()` guard used to produce; any other I/O error still
   propagates, preserving each function's existing "exists but unreadable
   is a real error" contract).
5. The two file categories the brief explicitly flagged (worktree lifecycle,
   `pricing.toml`/`models.toml`) were checked directly rather than assumed
   in scope — see §3.

## Verdict

**3 of 27 sites are TOCTOU-reachable; all 3 are fixed.** 4 sites are test
code (out of the reachability analysis entirely). The remaining 20
production sites are Not-reachable or Benign-by-design — mostly stack/runner
*detection* (`Cargo.toml`/`package.json`/`go.mod`/etc. existence checks that
pick which external test command to spawn next, with no subsequent read of
the checked file itself) where a stale answer changes which valid tool runs,
never what data is read.

## §1. Worktree lifecycle and `pricing.toml`/`models.toml` — checked, not assumed

The brief named these as warranting review by reachability. Neither turned
up a fixable site:

- **`crates/lopi-git/src/worktree.rs`** — zero `.exists()` calls in
  production code. `WorktreeManager::remove_orphan_dirs` already calls
  `tokio::fs::remove_dir_all` directly with no preceding existence check
  (a `NotFound`/other error is logged via `tracing::warn!` and the sweep
  continues); the primary removal path goes through `git worktree remove
  --force` (idempotent from git's side), not a raw check-then-delete. This
  file is already open-and-handle-the-error throughout — nothing to fix.
- **`crates/lopi-orchestrator/src/pool/worktree.rs`** — the one
  `.exists()` call (line 118) is a test assertion inside
  `#[cfg(test)] mod tests` (`checkout reaped`); the production functions
  (`setup_worktree`, `cleanup_worktree`) contain no `.exists()` calls at
  all — they delegate to `lopi-git`'s `Worktree::cleanup`/`Drop`, which is
  the file above.
- **`crates/lopi-agent/src/pricing.rs`** / **`crates/lopi-agent/src/model_config.rs`**
  — both `.lopi/pricing.toml` and `.lopi/models.toml` override loaders use
  `if let Ok(text) = std::fs::read_to_string(&path) { ... }` directly, with
  **no** `.exists()` call at all. This is exactly the open-and-handle
  pattern this phase asks for elsewhere — these two were already correct
  and don't appear in the 27-count for that reason.
- **`.konjo/` artifact paths** — grepped every file referencing `.konjo` in
  `crates/`/`src/` outside tests; none pairs it with `.exists()`. No
  `.konjo`-path TOCTOU site exists to fix.

## §2. Full inventory

| # | Path | Description | Reachable? | Status/Fix |
|---|------|--------------|------------|------------|
| 1 | `crates/lopi-tools/src/registry.rs:126` | `ToolRegistry::load` — `exists()` then `tokio::fs::read` on `$LOPI_HOME`/`~/.lopi/tool_registry.json` | Not-reachable | Lives outside any repo an agent edits; writer (`save_to_disk`) uses atomic tmp-file-then-`rename`, so once observed present the file never vanishes — worst case is a benign stale-empty read on first-ever write. No fix. |
| 2 | `crates/lopi-orchestrator/src/pool/worktree.rs:118` | `assert!(path.is_some_and(\|p\| !p.exists()), "checkout reaped")` | Test code | Inside `#[cfg(test)] mod tests` / `#[tokio::test]`. Out of scope. |
| 3 | `crates/lopi-ui/build.rs:19` | Build script — `dist.exists()` then `create_dir_all` | Not-reachable | Build-time only, single process, no agent/task runs during `cargo build`; `create_dir_all` is itself idempotent on "already exists". No fix. |
| 4 | `crates/lopi-agent/src/scorer.rs:125` | Picks `"./gradlew"` vs `"gradle"` by `repo_path.join("gradlew").exists()` | Benign-by-design | Decides only which binary name to spawn next line; nothing reads `gradlew`'s contents. Worst case falls back to a global `gradle` on `PATH` — not data corruption. No fix. |
| 5 | `crates/lopi-agent/src/scorer_detect.rs:52` | `Cargo.toml` existence → `Runner::Cargo` | Benign-by-design | Pure existence check selecting an enum variant (which test command to spawn); no subsequent read of the file. No fix. |
| 6 | `crates/lopi-agent/src/scorer_detect.rs:55` | `pnpm-lock.yaml` existence → `Runner::Pnpm` | Benign-by-design | Same shape as #5. No fix. |
| 7 | `crates/lopi-agent/src/scorer_detect.rs:58` | `yarn.lock` existence → `Runner::Yarn` | Benign-by-design | Same shape as #5. No fix. |
| 8 | `crates/lopi-agent/src/scorer_detect.rs:61` | `package.json` existence → `Runner::Npm` | Benign-by-design | Same shape as #5. No fix. |
| 9 | `crates/lopi-agent/src/scorer_detect.rs:67` | `go.mod` existence → `Runner::Go` | Benign-by-design | Same shape as #5. No fix. |
| 10 | `crates/lopi-agent/src/scorer_detect.rs:70` | `build.gradle` **or** `build.gradle.kts` existence (2 calls, 1 line) → `Runner::Gradle` | Benign-by-design | Same shape as #5. No fix. |
| 11 | `crates/lopi-agent/src/scorer_detect.rs:73` | `pom.xml` existence → `Runner::Maven` | Benign-by-design | Same shape as #5. No fix. |
| 12 | `crates/lopi-agent/src/scorer_detect.rs:92` | `is_python_project` — 5 manifest names via `.any(\|f\| ... .exists())` | Benign-by-design | Same shape as #5. No fix. |
| 13 | `crates/lopi-mcp/src/config.rs:91` | `load_servers` — `<repo>/.lopi/loop.toml` `exists()` then `read_to_string` | **TOCTOU-reachable** | `repo` is a working tree a concurrent agent/checkout/worktree-remove can mutate. A delete between check and read turned "exists" into `NotFound` and propagated `Err` instead of the intended empty-vec fallback. **Fixed** — read directly, classify `ErrorKind::NotFound` as "no servers", other errors still propagate via `anyhow::Context`. |
| 14 | `crates/lopi-spec/src/test_runner.rs:41` | `run_tests` — `Cargo.toml` existence → run `cargo test` | Benign-by-design | Selects which external process to spawn; nothing reads the checked file. No fix. |
| 15 | `crates/lopi-spec/src/test_runner.rs:43` | `setup.py` existence (part of Python-runner `\|\|` chain) | Benign-by-design | Same shape as #14. No fix. |
| 16 | `crates/lopi-spec/src/test_runner.rs:44` | `pyproject.toml` existence (same chain) | Benign-by-design | Same shape as #14. No fix. |
| 17 | `crates/lopi-spec/src/test_runner.rs:45` | `setup.cfg` existence (same chain) | Benign-by-design | Same shape as #14. No fix. |
| 18 | `crates/lopi-spec/src/lib.rs:160` | `SpecSurface::load` — `.lopi/spec_surface.json` `exists()` then `read_to_string` | **TOCTOU-reachable** | Written by `save()` (direct, non-atomic write) inside the repo an agent is editing; read during a **live agent run**'s seed step (`crates/lopi-agent/src/runner/seed.rs`) plus `lopi check`/`lopi gap-fill` CLI commands. A delete/rewrite race turns "cached surface" into a propagated `Err` instead of the intended "no cache" fallback. **Fixed** — read directly, classify `NotFound` as `Ok(None)`, other errors still propagate. |
| 19 | `crates/lopi-spec/src/lib.rs:354` | `tempdir()` test fixture — `path.exists()` then `remove_dir_all` | Test code | Inside `#[cfg(test)] mod tests` (starts line 248); filename doesn't end in `test(s).rs` so a naive filter misses it. Out of scope. |
| 20 | `crates/lopi-core/src/config.rs:351` | `RepoProfile::load_from_repo` — `.lopi.toml` `exists()` then `read_to_string` | Not-reachable | The subsequent chain (`.ok().and_then(...).unwrap_or_default()`) already maps **any** read/parse failure — including a race-induced `NotFound` — to the same `Self::default()` the `exists()` guard produces. Removing the guard would not change the outcome of any race. No fix (would be style-only, not a reachability fix). |
| 21 | `crates/lopi-core/src/config.rs:441` | `LopiConfig::find_and_load` — `lopi.toml` / `~/.lopi/lopi.toml` candidates | Not-reachable | lopi's own operator config, read once at CLI/process startup before any task or agent begins running — not a path inside a repo under active agent edit. No fix. |
| 22 | `crates/lopi-core/src/loop_config.rs:318` | `LoopConfig::load_from_repo` — `<repo>/.lopi/loop.toml` `exists()` then `read_to_string` | **TOCTOU-reachable** | The central loop-config load, called every attempt via `run_one`, against a repo/worktree a **concurrent agent, git checkout, or worktree removal** can be mutating — the sprint brief's own named example. A delete between check and read propagated `Err` (failing the whole task-run attempt) instead of the intended "no config → conservative defaults" fallback. **Fixed** — the primary, highest-consequence fix in this inventory; read directly, classify `NotFound` as `Ok(Self::default())`, other errors still propagate via `anyhow::Context`. |
| 23 | `crates/lopi-core/src/loop_config.rs:378` | `LoopConfig::validate` — `repo_path.join(v).exists()` for `vision_path` | Benign-by-design | Feeds only a human-readable warning string into a `Vec<String>`; nothing opens the file afterward. A stale answer is at most a misleading/missing warning, never a correctness or security issue. No fix. |
| 24 | `src/onboarding_import_commands.rs:279` | `assert!(!db_path.exists(), "must not open/create the store")` | Test code | Inside `#[tokio::test] async fn run_import_reports_zero_...`. Out of scope. |
| 25 | `src/loop_commands.rs:36` | `render()` — `path.exists()` purely to pick a display label ("config: `<path>`" vs "(none — showing defaults)") | Benign-by-design | Cosmetic only. The actual config load already happened via `LoopConfig::load_from_repo` on the previous line, independently of this check. No fix. |
| 26 | `src/spec_commands.rs:196` | `tempdir()` test fixture — `p.exists()` then `remove_dir_all` | Test code | Inside `#[cfg(test)] mod tests` (starts line 184). Out of scope. |
| 27 | `src/repo_detect.rs:13` | `find_git_root` — `current.join(".git").exists()` while walking up | Not-reachable | Decides only whether to stop walking upward; no read of `.git`'s contents follows. Runs once at CLI/REPL startup in the operator's own cwd. No fix. |

## Summary

- **27** `.exists()` lines found outside `crates/lopi-ui/src/web/*` and
  obvious test dirs/filenames — matches the brief's count.
- **4** of those are test code a filename-based filter can't catch
  (rows 2, 19, 24, 26) — excluded from reachability analysis.
- **23** production lines / **24** individual `.exists()` calls remain.
- **3 TOCTOU-reachable**, all fixed (rows 13, 18, 22) — every one is a
  config/artifact file living inside a repo an agent, a concurrent task, or
  a git operation can be actively mutating, matching the brief's own
  reachability criterion exactly.
- **20 Not-reachable or Benign-by-design**, left untouched per "fix only
  what the inventory shows is reachable" — mostly runner/stack *detection*
  (existence checks that pick which external command to spawn, with no
  subsequent read of the checked file) plus a handful of cosmetic/
  startup-only/outcome-equivalent checks.
- Worktree lifecycle (`crates/lopi-git/src/worktree.rs`,
  `crates/lopi-orchestrator/src/pool/worktree.rs`) and
  `pricing.toml`/`models.toml` — the two categories the brief flagged by
  name — were checked directly (§1) and found to already use
  open-and-handle-the-error (or, for worktree removal, git's own
  idempotent `remove --force`), so they contribute zero fixes here; this
  is a confirmed-clean finding, not an unreviewed gap.

## Not covered

This inventory covers `.exists()` call sites specifically — the
check-then-open/check-then-remove antipattern the brief named. It does
**not** cover: non-atomic *writes* that a concurrent reader could observe
mid-write (e.g. `SpecSurface::save`'s direct `std::fs::write`, not a
tmp-then-rename — a reader racing a writer can still see a truncated file
and get a JSON parse error, which is a different bug class from the
`exists()`-then-vanishes race this phase fixes); the ordering by which a
worktree's checked-out branch becomes visible to a *specific* task's
working directory (named as an explicit gap in `KT-S10.0`, still open);
or any `.exists()`-shaped check expressed differently (e.g. `Path::is_dir`,
`Path::try_exists`, `std::fs::metadata` used as an existence probe) — a
targeted follow-up grep for those idioms was not run as part of this phase.

## Files modified

- `crates/lopi-core/src/loop_config.rs` — `LoopConfig::load_from_repo`
  (row 22): check-then-open → read-and-classify-`NotFound`; added
  `use anyhow::Context;`.
- `crates/lopi-mcp/src/config.rs` — `load_servers` (row 13): same fix;
  added `use anyhow::Context;`.
- `crates/lopi-spec/src/lib.rs` — `SpecSurface::load` (row 18): same fix;
  extended `use anyhow::Result;` to `use anyhow::{Context, Result};`.

```
$ cargo build --workspace
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 01s

$ cargo test -p lopi-core -p lopi-mcp -p lopi-spec -p lopi-git -p lopi-orchestrator
... all suites: 0 failed

$ cargo clippy -p lopi-core -p lopi-mcp -p lopi-spec -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 11.37s
```
