# Branch Triage — 2026-08-11

Part E of the combined P3a-closeout / Collision-Oracle / RepoProfile-parity /
mutation-hunt-trigger / branch-triage sprint. Findings only — **no branches were
deleted**, per this sprint's explicit non-goal. Ages are relative to 2026-08-11.
lopi verdicts are based on `git for-each-ref --sort=-committerdate`, `git rev-list
--count` ahead/behind against `origin/main`, `git branch --merged`, and GitHub PR
state checked **per-branch, individually, via the GitHub API** — a first pass that
scanned only the 50 most recent closed PRs missed most of the real matches (PR
numbers here run as low as `#1`), so every row below was re-verified with a
head-scoped query rather than trusted from that incomplete scan.

## kiban — `claude/sign-distribution-channel-heg41d`

**Last commit:** 2026-07-26 (2 weeks stale). **Ahead/behind `main`:** 2 ahead, 28
behind. **`VERSION`:** branch still at `1.8.0`; `main` has moved to `1.14.0`.

**Real, non-superseded work — `security/allowed_signers` does not exist on `main`
at all.** The branch adds signed-release-tag verification to
`lib/self_update.sh` (kiban's self-distribution mechanism): a dedicated
release-signing keypair, `security/allowed_signers` as the trust anchor, an
updated `.github/workflows/release.yml` that cuts signed tags, and 11
`test_self_update.sh` cases (up from 6).

**A real `git merge-tree --write-tree origin/main
origin/claude/sign-distribution-channel-heg41d` was run — it does NOT apply
cleanly: exactly 4 conflicts, all in bookkeeping files**
(`CHANGELOG.md`, `LEDGER.md`, `NEXT_SESSION_PROMPT.md`, `VERSION`) — no conflict in
any of the substantive files (`lib/self_update.sh`, `security/allowed_signers`,
`.github/workflows/release.yml`, `docs/DISTRIBUTION.md`, `tests/test_self_update.sh`).
A rebase is mechanical, not risky.

**This branch had a real PR** — kiban #29, "Sign the distribution channel: verify
release tags before self-update applies them" — opened as a draft 2026-07-25 and
**closed (not merged)** on 2026-08-03. The PR body describes a completed, tested
implementation (11/11 `test_self_update.sh` cases, `ruff`/`mypy`/`pytest` all clean)
with one manual follow-up step outstanding (adding `RELEASE_SIGNING_KEY` as a GitHub
Actions secret). Git/PR history alone cannot say *why* it was closed rather than
merged — that's a fact for the repo owner, not something to infer here.

**Verdict: closed-PR-rebase-candidate.** The code is real, tested, and not
superseded, and a rebase past `main`'s 28 commits is mechanical (bookkeeping-only
conflicts) — but a closed PR is a decision point, not neglect, and reopening it is a
call for the repo owner, not an agent, per this sprint's own "report only" scope.

## lopi — `claude/*` and `lopi/*` branches

24 non-`main` branches (`origin/main`/`origin/HEAD` excluded), sorted newest-first.
"Merged into `main`" = `0` commits ahead per `git rev-list --count` (ref containment,
authoritative regardless of what a PR's own `merged` flag says — several of these
landed via a path GitHub doesn't mark as a button-merge, e.g. a squash or a
differently-numbered consolidating PR).

| Branch | Last commit | Age | Ahead/behind `main` | PR history (verified per-branch) | Verdict |
|---|---|---|---|---|---|
| `claude/sprint-p3a-planner-wiring` | 2026-08-09 | 2d | 8 / 0 | #195 open | active — Part A of this same combined sprint, owned by a concurrent session |
| `claude/colour-theme-implementation-ncnpf5` | 2026-07-31 | 11d | 4 / 24 | #189 open | active |
| `claude/dashboard-recon-sprint-u0-rfy9tl` | 2026-07-30 | 12d | 1 / 24 | #188 open | active |
| `claude/lopi-dashboard-recon-rnvvg7` | 2026-07-30 | 12d | 37 / 29 | #186 open (WIP) | active |
| `claude/konjo-cross-repo-work-6914op` | 2026-07-29 | 13d | 8 / 29 | #185 open | active |
| `claude/s13-quality-substrate-cua8oa` | 2026-07-29 | 13d | 2 / 54 | #183 **closed, not merged** | closed-PR — explicitly reviewed and rejected/superseded, not simply stale |
| `claude/budget-judge-hardening` | 2026-07-17 | 25d | 1 / 381 | #112 **closed, not merged** | closed-PR |
| `lopi/50cb6637-...-attempt-2` | 2026-07-11 | 31d | 1 / 462 | #85 **closed, not merged** | closed-PR — self-generated `lopi/<uuid>-attempt-N` branch ("count Rust source files") |
| `claude/prompt-templates-sprint-1` | 2026-07-06 | 36d | 0 / 547 | #54 closed, not merged | **already merged into `main`** (0 ahead) — content landed via a different path than PR #54's own merge button |
| `feat/macos-app-icon` | 2026-07-06 | 36d | 1 / 570 | #43 **closed, not merged** | closed-PR |
| `claude/pentad-m3-mcp-cli` | 2026-06-23 | 49d | 1 / 591 | #50 **closed, not merged** | closed-PR |
| `claude/konjo-lopi-aaju8e` | 2026-06-20 | 52d | 1 / 673 | **10 closed PRs** (#27–#37), none merged | closed-PR — this branch name was reused/re-pushed across 10 separate PR cycles |
| `wesley/forge-ui-overhaul` | 2026-06-10 | 62d | 6 / 706 | none found | true no-PR abandoned — the one branch in this list that never had a PR at all |
| `lopi/macos-web-parity` | 2026-06-10 | 62d | 2 / 707 | none found | true no-PR abandoned |
| `lopi/1f5dfb7e-...-attempt-1` | 2026-06-09 | 63d | 2 / 713 | #19 **closed, not merged** | closed-PR — self-generated attempt branch ("print hello to a file") |
| `lopi/a437ae4f-...-attempt-1` | 2026-06-09 | 63d | 1 / 713 | none found | true no-PR abandoned — self-generated attempt branch ("say hello") |
| `lopi/a5d80420-...-attempt-2` | 2026-06-01 | 71d | 1 / 718 | #18 **closed, not merged** | closed-PR — self-generated attempt branch |
| `claude/telegram-bot-overhaul-8iJpe` | 2026-06-01 | 71d | 0 / 721 | 4 closed PRs (#14–#17), none merged | **already merged into `main`** (0 ahead) |
| `claude/konjo-lopi-Ad8Ch` | 2026-05-19 | 84d | 1 / 746 | #13 **closed, not merged** | closed-PR |
| `claude/plan-konjo-self-modify-6LyQv` | 2026-05-13 | 90d | 23 / 757 | #8 **closed, not merged** | closed-PR |
| `claude/elastic-merkle-385c20` | 2026-05-10 | 93d | 8 / 776 | none found | true no-PR abandoned |
| `claude/sprint-i-memory` | 2026-05-08 | 95d | 0 / 785 | #6 closed, not merged | **already merged into `main`** (0 ahead) |
| `claude/advance-all-repos-T5j3q` | 2026-05-08 | 95d | 1 / 791 | #2 **closed, not merged** | closed-PR |
| `claude/add-lopi-tests-SqAUe` | 2026-05-08 | 95d | 0 / 801 | 4 closed PRs (#1, #3, #4, #5), none merged | **already merged into `main`** (0 ahead) |

### Summary — corrected after per-branch verification

The first pass over this table guessed "no PR" for most of these branches from an
incomplete 50-result closed-PR scan. Re-checked individually, that guess was wrong
for all but 3: **almost every branch in this repo's long tail was opened as a PR and
explicitly closed, not simply forgotten.**

- **5 branches with an open PR** — actively in review, no action needed from this
  triage.
- **4 branches already fully merged into `main`** (`0` ahead, confirmed via
  `git rev-list --count`, independent of what their old PR's `merged` flag shows) —
  safe deletion candidates, since their content already lives on `main`. Not deleted
  here per this sprint's non-goal.
- **12 branches have a closed-but-unmerged PR** — a real prior review decision, not
  neglect. Reopening any of them (kiban's `sign-distribution-channel-heg41d` is the
  same shape) is a call for the repo owner, not something this triage decides.
  `claude/konjo-lopi-aaju8e` stands out with 10 separate closed PR cycles against the
  same branch name.
- **3 branches genuinely never had a PR**: `wesley/forge-ui-overhaul`,
  `lopi/macos-web-parity`, `lopi/a437ae4f-...-attempt-1` — these are the only true
  "opened, worked on, never reviewed" cases in the set.

No deletions performed. Verdicts are this triage's read of the evidence, not a
decision — that stays with whoever owns branch cleanup next.
