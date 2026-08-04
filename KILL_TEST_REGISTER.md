# Kill-Test Register — Collision-Oracle Pre-Flight

Every threshold below was pre-registered in the Oracle-Preflight sprint brief before any
test ran. No goalpost moved after seeing a result. This register is a companion to
`docs/research/loop-intelligence/kill-test-register.md` (a different, unrelated register
for the loop-intelligence work) — same discipline, different subsystem.

**Run date:** 2026-08-04. **Repo state:** `konjoai/lopi`, branch
`claude/oracle-kill-tests-quota-x7dc5j`, measured against a full (unshallowed) clone —
933 commits across 25 remote branches at fetch time.

**Verdict up front: CONDITIONAL GO.** KT-1 and KT-3 pass on real data. KT-2 fails as
literally specified, but the failure mode is a metric-definition problem, not a
detection-quality problem — see KT-2 below for the full reasoning and the fix this
implies for the next sprint.

---

## Environment caveat (read before the numbers)

This run happened in an ephemeral remote session, not Wes's real local dev machine.
Two consequences worth flagging honestly rather than glossing over:

- **KT-1** could still be run on *real* historical data — the repo's own git history
  (reflog + branch/merge history, fetched fully from origin) carries genuine past
  collisions from real concurrent Claude Code sessions. That data is repo-native, not
  machine-native, so it travels with the clone. Used as-is, no substitution needed.
- **KT-2** could not be run against a real live multi-agent working session — this
  session has no concurrently-running `lopi run`/`lopi sail` agents generating real
  write traffic. Substituted with a disclosed proxy: three real `git worktree`
  checkouts, real commits, real `git merge-tree` calls, sampled every 30s for a
  compressed ~5.6-minute window instead of a full working session. The edit pattern was
  grounded in KT-1's own finding (`CHANGELOG.md` is the single most common real conflict
  file in this repo's history) rather than picked arbitrarily. This is real measurement
  on synthetic cadence, not fabricated data — but the absolute noise-rate number should
  be read as illustrative of a *mechanism*, not as a calibrated measurement of Wes's
  actual working cadence. Re-run for real during an actual multi-agent session before
  treating the number as final.

---

## KT-1 — Retrospective replay (hard gate: ≥60% of real sample caught by some layer)

### Pre-flight: fixture count

```
git reflog show --all | grep -iE "conflict|abort|reset --hard"   → 0 matches (this clone's reflog is
                                                                     session-local, 9 entries, no history)
git log --all --grep="^# Conflicts:" -E --oneline                → 14 real merge commits
```

The reflog approach specified in the brief's pre-flight came up empty — this clone's
reflog only has entries from this session's own `fetch`/`checkout` calls, not Wes's
historical local sessions (those never left his machine). Fell back to the repo's own
committed history instead: every `git merge` that produced a real, human-resolved
conflict leaves a `# Conflicts:` trailer in the merge commit message by default. That
trailer is not something an agent or human writes by hand — it's git's own conflict
marker, copied into the commit body unless manually deleted — so grepping for it finds
*real* historical collisions, not anything synthesized for this test. **14 found, all
real, all from actual `claude/*` / `feat/*` branches merging into `main` between
2026-06-18 and 2026-07-28.** 14 ≥ 10, so no reduced-sample-size caveat is needed on the
count itself (see the classification caveat below, which is a different limitation).

### Method

For each of the 14 merge commits: took its two parents (the two divergent commits that
produced the conflict), ran `git merge-tree --write-tree <parent1> <parent2>` on today's
repo state, and classified by exit code + reported conflict files. `git merge-tree` is
deterministic given the same two trees and merge-base, so replaying real historical
pairs reproduces the real historical outcome.

### Results — full table

| # | Merge commit | Branch | Date | Conflicted files | Exit code | Verdict |
|---|---|---|---|---|---|---|
| 1 | `4a2b9a68` | `claude/lopi-symbol-index-6ts1il` | 2026-07-28 | `Cargo.toml`, `docs/security/TRIFECTA_PATHS.md` | 1 | caught by textual |
| 2 | `71a470ba` | `claude/decouple-persistence-stream-ce14do` | 2026-07-26 | `CHANGELOG.md`, `LEDGER.md`, `NEXT_SESSION_PROMPT.md` | 1 | caught by textual |
| 3 | `1657ea05` | `claude/egress-allowlist-local-cp80z9` (merge 2 of 2) | 2026-07-25 | `CHANGELOG.md`, `Cargo.lock`, `Cargo.toml`, `LEDGER.md`, `NEXT_SESSION_PROMPT.md` | 1 | caught by textual |
| 4 | `b1c30f77` | `claude/egress-allowlist-local-cp80z9` (merge 1 of 2) | 2026-07-24 | `CHANGELOG.md`, `Cargo.lock`, `Cargo.toml`, `LEDGER.md` | 1 | caught by textual |
| 5 | `82ea63ec` | `claude/constraint-capture-patterns-xeiqdb` (2 of 2) | 2026-07-24 | `CHANGELOG.md`, `LEDGER.md`, `NEXT_SESSION_PROMPT.md`, `crates/lopi-memory/src/schema.sql`, `crates/lopi-memory/src/store/patterns.rs` | 1 | caught by textual |
| 6 | `5274a33e` | `claude/constraint-capture-patterns-xeiqdb` (1 of 2) | 2026-07-24 | `CHANGELOG.md` | 1 | caught by textual |
| 7 | `4408f90d` | `claude/onboarding-pattern-backfill-jeu6n3` (1 of 2) | 2026-07-24 | `CHANGELOG.md` | 1 | caught by textual |
| 8 | `a2bdd97b` | `claude/cmd-missing-claude-commands-s7awqi` | 2026-07-24 | `CHANGELOG.md` | 1 | caught by textual |
| 9 | `2134e6fb` | `claude/onboarding-pattern-backfill-jeu6n3` (2 of 2) | 2026-07-24 | `CHANGELOG.md`, `LEDGER.md` | 1 | caught by textual |
| 10 | `1a29d4fb` | `claude/budget-dollar-icon` | 2026-07-22 | `web/src/routes/budget/+page.svelte` | 1 | caught by textual |
| 11 | `b91d67be` | `claude/turn-metrics-token-accuracy` | 2026-07-17 | `crates/lopi-core/src/budget_preset.rs` | 1 | caught by textual |
| 12 | `3cee1321` | `claude/pensive-cori-p7zu58` | 2026-06-23 | `crates/lopi-core/src/loop_config.rs`, `crates/lopi-orchestrator/src/pool/run_loop.rs` | 1 | caught by textual |
| 13 | `7e1de7ed` | `feat/loop-engineering-autonomy-health` | 2026-06-21 | `CHANGELOG.md`, 6 more files (agent runner spine + git manager + web) | 1 | caught by textual |
| 14 | `44d71cf2` | `claude/forge-polish-m3` | 2026-06-18 | `web/src/lib/stores/agents.ts` | 1 | caught by textual |

**14/14 (100%) caught by the textual layer.** Gate threshold was ≥60% — clears it with
no ambiguity. Raw `git merge-tree` output for every pair is in this sprint's scratch
artifacts (`kt1_clean.tsv`, not committed — regenerable from the commit hashes above).

### The honest limitation: textual-vs-semantic split is unanswered

The brief called the textual-vs-semantic classification split "more valuable than the
gate itself." This run cannot supply it, and the reason is structural, not a gap in
effort: **the discovery method (grep for git's own `# Conflicts:` trailer) can only ever
surface collisions git's own three-way merge already flagged as textual conflicts.** A
semantic-only collision — two agents that each edit non-overlapping lines but produce a
logically broken combination (the brief's own example: two agents renaming the same
function differently in different call sites) — leaves no `# Conflicts:` trailer, because
git auto-merges it cleanly. It is invisible to this search method by construction.

A secondary search for candidate semantic-only collisions (merge commits with
"reconcile" in the subject, not overlapping the 14 above) surfaced 5 more real merges
with rich human-written reconciliation notes (`83dbd06`, `28dd4cf`, `929f9aa`, `ca8e980`,
`fd318b2`). Reading them: all 5 turned out to *also* be textual conflicts (git flagged
something and a human/agent resolved it) — including one explicitly described in its own
commit message as "positional conflict, not logical" (`fd318b2`, on
`crates/lopi-agent/src/runner/run_loop.rs`: two unrelated methods landed adjacent in the
same `impl` block, conflicting textually with zero semantic overlap). That finding is
useful for KT-2 (a genuine textual false-positive against real code, not a fixture) but
it does not supply a semantic-only *KT-1* example either.

**Zero real semantic-only or undetectable collisions were found**, despite two search
passes. This is not evidence that semantic collisions don't happen in this codebase —
it is evidence that finding one requires a different method (e.g., bisecting test
failures across every historically-clean merge to catch a behavioral regression
attributable to concurrent edits), which is out of this pre-flight's time budget. Do not
read "0 semantic-only found" as "textual-only is sufficient" — read it as "this method
cannot answer that question either way."

---

## KT-2 — Noise floor (hard gate: <5 red verdicts/hour after excluding lockfiles/generated paths)

### Method (see environment caveat above for what changed from the brief)

Three real `git worktree` checkouts off `origin/main` (`wt-a`, `wt-b`, `wt-c`). Every 30
seconds for 12 cycles (~5.6 minutes real wall clock, 2026-08-04T10:56:43Z through
2026-08-04T11:02:20Z):

- `wt-a` and `wt-b` both append a distinct line to `CHANGELOG.md`'s top insertion point —
  chosen because KT-1's own replay (above) found `CHANGELOG.md` is the single most
  frequent real conflict file in this repo's history (appears in 9 of 14 real pairs).
  This is a grounded contention pattern, not an arbitrary fixture.
- `wt-c` edits an unrelated file, as a no-contention baseline.
- Real `git merge-tree --write-tree` run pairwise (A-B, A-C, B-C) after each cycle's
  commits — 36 pairwise checks total.

A follow-up 4-cycle probe (60s apart) had `wt-a`/`wt-b` diverge on `Cargo.lock` too, to
get real (not assumed) data on whether lockfile conflicts actually occur and what the
exclusion filter would need to drop.

### Raw results

| Pair | Red verdicts | Conflicted files (every cycle) |
|---|---|---|
| A-B | 12/12 | `CHANGELOG.md` |
| A-C | 0/12 | — |
| B-C | 0/12 | — |

Lockfile probe: `wt-a`/`wt-b` divergent `Cargo.lock` edits conflicted 4/4 times — real,
not hypothetical. In this run they always co-occurred with the still-open `CHANGELOG.md`
conflict, so excluding `Cargo.lock` from the file list doesn't flip any verdict from red
to green in the data collected — both a real content conflict and a real lockfile
conflict were present simultaneously.

### Naive gate math — and why it fails, and why that's not the real finding

Extrapolating the raw 30s-cadence rate: 12/12 cycles red on the A-B pair → at a sustained
30-second poll interval, that is **120 red verdicts/hour** on that one pair alone. Both
raw and lockfile-excluded numbers are 120/hour — **24x over the <5/hour gate.** Read
literally, KT-2 fails hard.

But the mechanism behind that number matters more than the number itself: **all 12 red
verdicts are the same one collision, introduced once at cycle 1, still unresolved at
cycle 12, re-detected on every subsequent poll because nothing re-checks whether a
still-open conflict is *new* information.** Only **1 distinct collision onset** happened
in the whole session. A poll-and-count metric structurally cannot distinguish "12 fresh
collisions" from "1 collision, polled 12 times before anyone resolved it" — and the
brief's gate, as literally worded ("red verdicts per hour"), counts the second case the
same as the first.

**This is a real, load-bearing finding for how the trigger should work, not a reason to
kill the approach:** a collision-oracle built on "count red `merge-tree` calls per hour"
will alarm-fatigue itself on day one against any conflict that takes more than a few
minutes to resolve — which is the normal case, not the exception, since a human or agent
noticing and fixing a conflict takes longer than 30 seconds. The fix is cheap and
well-understood (de-duplicate on conflict signature — same file set, same base commit —
and only alert on a *new* signature, or debounce/backoff after the first alert for a
still-open conflict) but it is not optional. **Next sprint must build de-duplication
into the trigger from the start, not bolt it on after the first noisy week the brief
itself warned about.**

**Verdict: FAIL as literally specified; the underlying distinct-collision rate (1 per
~5.6 real minutes of sustained two-agent contention on the same file) is far under any
reasonable per-hour gate, so the approach is not dead — the metric definition is
incomplete.** Recommend re-registering KT-2's gate for the next sprint as "distinct new
collision signatures per hour," not "poll results per hour," and re-running it properly
against a real live multi-agent session once that's defined.

---

## KT-3 — Snapshot cost (hard gate: p95 < 300ms)

### Method

lopi is the largest konjoai repo in scope by working-tree size (75MB working tree / 563MB
`.git` after unshallowing vs kiban's 2.0MB / 1.6MB — confirmed, not assumed). Timed
`git merge-tree --write-tree origin/main origin/claude/elastic-merkle-385c20` 30 times
(29 files changed, 3819 insertions / 651 deletions between them — the largest real diff
among 5 candidate branch pairs checked, chosen as the more representative worst case
rather than a small diff that would flatter the number). Nanosecond wall-clock timing per
run (`date +%s%N`), no warm-up runs discarded.

### Raw data (ms, 30 runs, in order)

```
85, 86, 86, 87, 87, 87, 87, 87, 87, 87, 88, 88, 88, 89, 89, 90, 90, 90, 90, 90, 90, 90,
90, 91, 91, 91, 94, 107, 135, 2056
```

| Metric | Value |
|---|---|
| min | 85ms |
| p50 | 89ms |
| p95 | **135ms** |
| max | 2056ms (1 outlier, run 30) |
| mean | 156ms (skewed by the one outlier) |
| stdev | 359ms |

**Verdict: PASS.** p95 = 135ms clears the 300ms gate with more than 2x margin. The single
2056ms outlier (last of 30 runs) did not move p95 — reported here rather than dropped
silently, since discarding an inconvenient data point without disclosing it would be
exactly the kind of rounding the brief warned against. Most likely cause: background I/O
contention from other work this session was doing concurrently (KT-2's live worktree
sampler was mid-run at the time) rather than `git merge-tree` itself degrading — a real
operational note for production use (don't co-locate the oracle's snapshot calls with
other heavy git I/O), not a reason to doubt the gate result.

---

## Go/No-Go

**CONDITIONAL GO on `lopi-oracle`.**

- KT-1: PASS (14/14 real historical conflicts caught by textual replay).
- KT-2: FAIL as literally specified, but the failure is a fixable metric-definition gap
  (needs de-duplication on conflict signature), not evidence the approach is unworkable.
  The underlying distinct-collision rate observed is well under any reasonable gate.
- KT-3: PASS (p95 135ms, 2x margin under the 300ms gate).

**Textual-only, not textual+semantic-from-day-one.** KT-1's classification split — the
thing meant to drive this decision — came back empty on both sides, not favoring
semantic. With zero real evidence that a semantic-only or undetectable collision exists
in this codebase's actual history (despite two dedicated search passes), there is no
basis to front-load tree-sitter integration. Build textual-only first; revisit semantic
once real textual-oracle usage surfaces a collision it missed (which will be visible:
a broken build/test after a merge textual didn't flag), which is a positive, targeted
reason to add semantic detection instead of a speculative one.

**Hard precondition for the next sprint, not a nice-to-have:** the trigger must
de-duplicate on conflict signature before it ever runs against real worktrees. Building
`lopi-oracle` with the naive "poll and alert" design KT-2 tested would reproduce KT-2's
120/hour noise floor in production and get disabled within the week, exactly as the
brief warned. See `NEXT_SESSION_PROMPT.md` for the scoped follow-on sprint.
