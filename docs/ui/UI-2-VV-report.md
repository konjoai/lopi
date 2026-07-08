# UI-2 V&V Report — Loop Stack (`/stacks`)

**Under test:** PR #64 (merged to `main` as `55338d5`), audited on `chore/ui-2-vv` from `origin/main` at that same commit.
**Auditor scope:** read-only verification pass + coverage-gap-closing tests only. No shipped-code defects were found this audit, so no functional fixes were needed.

## Go / no-go

> **GO for the backend phase**, with two escalations for Wes to decide (§4) — neither invalidates the UI-2 diff's own correctness. One is pre-existing repo-wide CI policy debt unrelated to PR #64; the other is a visual-honesty nuance (a badge that reads as enforced when nothing is enforced) that doesn't affect data or execution.

## 1. Hard gates

| Gate | Verdict | Evidence |
|---|---|---|
| **G1** — tests/check/build green, no skip/loosen, no `continue-on-error` | **PASS (frontend) / FAIL (repo CI policy, pre-existing)** | `npm test`: **444/444** passed, 0 failed, across 13 files (counted individually, see §2A). `npm run check`: 0 errors, 2 pre-existing warnings in files this PR never touched (`HelpOverlay.svelte`, `fleet/+page.svelte`). `npm run build`: succeeds. Grep for `.only(`/`.skip(`/`xit(`/`xdescribe(` across `web/src/**/*.test.ts`: **zero hits**. **However:** `.github/workflows/konjo-gate.yml` has **11** `continue-on-error: true` instances (clippy pedantic, cargo audit, cargo deny, coverage run, coverage gate, rustdoc missing-docs gate, and 5 in the Wall-3 adversarial-review job — including the job's own "fail if BLOCKER verdict" step). All 11 **pre-date PR #64** (confirmed: `grep -c continue-on-error .github/workflows/konjo-gate.yml` = 11 on both `c0f9398` and `55338d5`) and are Rust/repo-wide, not introduced by or specific to the UI-2 diff. Escalated in §4, not fixed. |
| **G2** — every WIRED field round-trips correctly, incl. `maxIterations: 0 ⇒ ∞` | **PASS** | `stack.test.ts`'s new table-driven block ("V&V: table-driven WIRED round-trip (§C)") — 9 rows covering model/effort/repo/gate/until/on_fail(×2)/maxIterations(7 and 0), each asserted via `eqIs`, plus a key-completeness assertion (`eq(keys, ['effort','gate','max_iterations','model','on_fail','until'], ...)`) proving no WIRED field is silently dropped or renamed. `cardToTaskPayload` at `stack.ts:585`. |
| **G3** — CLIENT-ONLY/STUBBED controls are provably inert | **PASS, with one visual-honesty caveat** | The same key-completeness test proves `budget`/`branch`/`autonomy` never appear in the emitted `CreateTaskOptions`. `EvalsPopover.svelte`: grep for `pass`/`fail`/`score` finds only the file's own honesty-rule doc comment — no execution state rendered anywhere. Baseline lock double-guarded: `toggleEval` (`stack.ts:405`, `if (name === BASELINE_EVAL.name) return evals;`) and the UI (`EvalsPopover.svelte`, `disabled={locked}` + early-return in `toggle()`). **Caveat:** `StackConnector.svelte:31` renders a badge — `⏸ budget 200k` — between cards whenever `guardrails.budget !== 'auto'`. Styled identically to the real (WIRED) schedule cadence badge and using a pause icon, it visually reads as an enforced limit though nothing enforces it. See escalation §4.1. |
| **G4** — `/loop` untouched; `/stacks` console-clean | **PASS** | `git diff c0f9398 55338d5 --stat -- web/src/routes/loop web/src/lib/components/AgentPane.svelte web/src/lib/components/Composer.svelte web/src/lib/components/LaunchControls.svelte` → **empty** (zero files, zero lines). Full `git diff c0f9398 55338d5 --name-only` lists exactly 23 files, all under `web/src/lib/components/stacks/`, `web/src/lib/stores/stack*.ts`, `web/src/routes/stacks/+page.svelte`, or root docs (`CHANGELOG.md`/`LEDGER.md`/`NEXT.md`/`UI_PLAN.md`) — nothing else. Console-clean confirmed across: initial load, composer add/prepend, iteration pill hover-stepper, all three popovers open/close/edit, config drawer, connector hover + insert-between, duplicate/delete/drag, empty→single→multi-card transitions, Esc/outside-click/scroll-close, and narrow-viewport bottom-sheet — the only console entries anywhere were the expected `/api/repos` + WebSocket `ECONNREFUSED` noise from no backend being present (verified identical on the untouched `/loop` route too, so it's not a regression). |
| **G5** — no new test-runner dependency | **PASS** | `grep -iE "playwright|vitest|jest|cypress|puppeteer|@testing-library" web/package.json` → no hits. The audit's own screenshot/interaction script (`vv-shots.mjs`) used a **pre-installed system Playwright** (`/opt/node22/lib/node_modules/playwright`, symlinked into `web/node_modules/playwright` only for the duration of the audit) — never added to `package.json`, and both the symlink and the script were deleted before finalizing this branch. |

## 2. Findings (§3 checklist)

### A. Test-suite integrity
| Item | Verdict | Evidence |
|---|---|---|
| Reported test count, per file | PASS | `parser.test.ts` 78 · `forge/connections.test.ts` 18 · `forge/excitement.test.ts` 24 · `api.test.ts` 23 · `ui/badges.test.ts` 18 · `stores/events.test.ts` 14 · `stores/layout-core.test.ts` 32 · `stores/agentReducer.test.ts` 41 · `stores/session-groups.test.ts` 16 · `stores/transcript.test.ts` 22 · `render/markdown.test.ts` 11 · `forge/orbState.test.ts` 26 · `stores/stack.test.ts` **121** (was 103 at PR #64 merge; +18 added this audit to close gaps in §2B/§2C). **Total: 444**, 0 failed. Each file uses the repo's plain assert-and-tally harness (`$lib/test-harness`); none is a mock-against-itself no-op — spot-checked `stack.test.ts` and `transcript.test.ts` line by line, both assert real transform outputs against literal expected values. |
| `.only`/`.skip`/`xit`/`xdescribe` | PASS (none) | `grep -rnE "\.only\(|\.skip\(|\bxit\(|\bxdescribe\(" web/src --include=*.test.ts` → no hits. |
| `continue-on-error` | FAIL (pre-existing, not #64) | See G1 above. |
| No tests deleted in #64 to make gates pass | PASS | `git diff c0f9398 55338d5 --name-status -- '*.test.ts'` touches **only** `stack.test.ts` (202 insertions, 4 deletions). The 4 removed lines are a stale import, the old `card()` helper body (replaced by a `buildCard`-based equivalent, same contract), a re-stated section comment, and one line rewritten in place to add more assertions — zero net assertion loss, confirmed by diffing the removed vs. added lines directly. |

### B. Store correctness
| Item | Verdict | Evidence |
|---|---|---|
| `add` prepends | PASS | `addCard` = `[card, ...cards]`; test "add prepends to top". |
| `insert(stackKey, index, loop)` lands at exact index | PASS | `insertIntoPane` (`stack.ts:640`) → test "insertIntoPane inserts into the named pane at the given index". |
| `duplicate` clones idle, fresh id, no `taskId`/output | PASS | `duplicateCard` (`stack.ts:340`) resets `status`/`iteration`/`taskId`; tests assert all three explicitly, using a fixture that starts `status:'running'` with progress + a `taskId` to prove the reset actually fires (not just "already undefined"). |
| `remove` removes only the target | PASS | Tests "remove drops the matching card" / "no-op for an unknown id". |
| Reorder is within-one-stack only | PASS (was a coverage gap — closed) | **Gap found:** the shipped tests exercised `reorderCard`/`moveCardBeforeOrAfter` only as raw array functions, never through the pane-keyed dispatch (`applyToPaneCards`) the real UI actually calls — so "reorder never touches the other pane" was asserted for insert, never for reorder. **Closed:** added two tests ("reorder via applyToPaneCards affects only the named pane" / "drag-relative reorder... affects only the named pane"), both asserting the *other* pane's array keeps object identity (`===`) across the operation. Structurally, cross-pane reorder is also inexpressible — `reorderInPane`/`reorderInPaneRelative` (`stack.ts`) each take one `key`; there is no exported op accepting two. UI-level: `StackCard.svelte`'s `onDragOver`/`onDrop` both early-return on `cur.paneKey !== paneKey`. |
| `maxIterations` floor 2, wraps to ∞, shared by pill + guardrails | PASS | `stepMaxIterations` (`stack.ts:428`) tested for all four edge transitions (down through floor, multi-step down, up from ∞ skips to floor not 1, down from ∞ stays ∞). Sharing confirmed by source: `StackCard.svelte` and `GuardrailsPopover.svelte` both call the identical `updateCardInPane(paneKey, card.id, { maxIterations: stepMaxIterations(card.maxIterations, delta) })` against the same reactive `card` prop — no component-mount tooling exists in this repo to assert this at runtime (confirmed no Vitest/Testing-library — see G5), so this is a source-citation proof, the strongest form available under this repo's test-tooling constraints. |
| Pane keys stable across ops | PASS (was a gap — closed) | Same fix as the reorder row above; `applyToPaneCards` (`stack.ts:625`) itself is also directly tested ("applyToPaneCards composes with any pure card-list op"). |

### C. WIRED round-trip
| Item | Verdict | Evidence |
|---|---|---|
| Table-driven proof, gate/until/on_fail/maxIterations(incl. 0)/model/effort/repo | PASS | New table-driven block in `stack.test.ts` (9 rows + key-completeness assertion) — see G2. |
| Schedule cron two-way sync (preset→string, string→preset/custom) | PASS at the format level; **weaker proof than the task-payload fields** | `buildCronString`/`cronHuman` tested for every `freq` (every-minute/hourly/daily/weekly/custom), including "custom cron passes raw through." **Gap found and closed:** no test previously proved a custom-flagged cron that happens to *numerically match* a preset's shape stays "custom" rather than snapping — added `cronHuman({...defaultCron(), freq:'custom', raw:'0 2 * * *'})` → `'custom cron'` (not "every day at..."). Source-level: `SchedulePopover.svelte`'s `onRawInput` unconditionally sets `freq:'custom'` on any raw edit — there is no reverse-detection code path to regress. **The narrower-proof note:** unlike `cardToTaskPayload`, there is **no `cardToSchedulePayload`-equivalent function** mapping a `StackCard` into the real `ScheduleBody`/`createSchedule()` shape (`{name, cron, goal, repo, priority, enabled}`) — the WIRED claim for cron is proven at "the string format `ScheduleEntry.cron` expects," not "this exact object round-trips through `createSchedule()`." Building that function requires a design decision (`StackCard` has no `name` field; `ScheduleBody.name` needs one synthesized) that's out of this audit's "trivial fix" scope — escalated in §4.2. |
| Reverse mapping (task/schedule → card load path) | N/A, confirmed absent | Grepped for any code loading a `StackCard` from a `Task`/`Schedule` response — none exists. Consistent with stacks being purely client-only/in-memory this slice; not a defect, just confirmed absent rather than assumed. |

### D. CLIENT-ONLY honesty
| Item | Verdict | Evidence |
|---|---|---|
| `budget`/`branch`/`autonomy` persist in store, absent from task payload | PASS | Key-completeness test (§C/G2); each field carries a `// TODO(backend)` (`stack.ts:45-47`, `:58`, `:94-97`, `:102-103`). |
| Evals: no pass/fail/score rendered; baseline locked | PASS | See G3. |
| Budget: enforces nothing; reads as enforced? | **Flagged** | Confirmed store-state-only (`patchGuardrails({budget: b})` in `GuardrailsPopover.svelte`, never read by `cardToTaskPayload`). **Does currently read as enforced**: the `StackConnector` badge (see G3 caveat) and the guardrails-summary line's `budget:200k` text both present it inline with WIRED guardrail text (`gate`, `until`, `max N`) with no visual distinction marking it as not-yet-enforced. See escalation §4.1 for the hide-vs-keep decision this needs. |

### E. STUBBED honesty
| Item | Verdict | Evidence |
|---|---|---|
| Run-menu items mutate nothing real | PASS | `RunMenu.svelte`'s `pick()` calls only `onClose()`. `StackPane.svelte`'s `runNow()` sets a local boolean only. Grepped both files for `createTask`/`createSchedule`/`fetch(` — zero hits. |
| `StackOutput` gated on real `taskId`; stays absent in normal use | PASS | `StackPane.svelte:75`: `{#if card.status === 'running' && card.taskId}`. Grepped the entire `stacks/` component tree and `stack.ts` for any code that assigns `card.status = 'running'` or a real `taskId` outside of `duplicateCard`'s explicit reset-to-`undefined` — **none exists**. Confirms `StackOutput` is structurally unreachable in the shipped app today, exactly as claimed. |
| `StackOutput` reuses `transcript.ts`, not a parallel feed | PASS | `StackOutput.svelte:12`: `import { transcripts, ... } from '$lib/stores/transcript'`. |
| `StackOutput` structure correct when fed a real feed | PASS | Fed a fixture `task_id` through the **real** `recordTranscript()` API (task_started, a `💭`-prefixed thinking log line, a status change, a tool_call/tool_result pair, and a plain output log line) via a temporary, reverted debug hook (see §5 methodology note). Screenshot `01-idle-queued-running.png` (not committed — see below) shows the collapsed strip correctly displaying the *actual* latest block's kind (`output`, not a hardcoded `thinking`) and text, matching the mockup's collapsed-strip contract. |

### F. Visual parity
Captured via a throwaway, uncommitted Playwright script (`web/vv-shots.mjs`, deleted after use — see G5) against `npm run build && npm run preview`. Screenshots were reviewed inline during the audit and are **not committed** (ephemeral verification artifacts, per the brief's "no new deps / throwaway script" instruction — the evidence below is the citation).

| State | Verdict | Evidence |
|---|---|---|
| Idle card (no summary lines/separator) | PASS | Visual: plain card, just alias/spec + cardbar, no `<hr class="sep">` rendered (`showSep` false when nothing active). |
| Queued (scheduled line + dotted connector + cadence badge) | PASS | Visual match; dotted `.cline-full`, centered `.connbadge.sched` reading "every day at 2:00 AM." |
| Running (iteration bar, flashing block) | PASS | 3-segment bar rendered as done/current/pending per `card.iteration={current:2,total:3}`; runtag "RUNNING · ITER 2/3." Flash confirmed via **computed style**, not just a static screenshot (see below). |
| Schedule popover: enable, combo hour/min, AM/PM, cron sync, next-runs, first-open anchor | PASS | `boundingBox()` on first open: `{x:148.9, y:586.5, w:300, h:139}` — **not (0,0)**, confirming the PR #64 positioning-bug fix holds on a fresh build. Weekly cadence screenshot shows correct dow/hour/min/AM-PM controls and synced raw cron. |
| Guardrails popover | PASS | Gate/until toggles, shell inputs, on-fail + budget segmented controls, max-iter stepper all render and update per interaction. |
| Evals popover | PASS | Flat checklist, tier badges (BASE/TEST/JUDGE/SUITE), KCQF suite shortcut correctly turns on its 4 named evals (5 total incl. baseline). |
| Config drawer: 5 selectors, wraps as group at narrow width | PASS | At 480px viewport, 5 chips wrap 3+2, none stretched/spread. |
| Connector hover: insert-between at right index | PASS | Script asserted card count 2→3 after clicking the revealed insert block; visual shows the dashed cyan "+"" block centered on the gap. |
| Live output: collapsed→expanded, static separator, orange outline | PASS | See §E; separator above output uses a plain, non-animated border (confirmed via computed style — no `animation-name` on `.pc`'s bottom border, only the two elements the spec calls out flash). |
| 5s flash respects `prefers-reduced-motion` | **PASS, strong evidence** | `getComputedStyle` on `.pc.running`, `.output`, `.iterbar i.cur`: without reduced motion → `animationName` = `cardflash` / `...outflash` / `...pulse` (real animations, confirming the media query isn't just always-off). With `reducedMotion:'reduce'` emulated → all three report `animationName: 'none'`. This is the single strongest-evidenced item in the whole audit. |
| Two panes: independent, responsive, drag stays in-stack | PASS | 1600px: side-by-side. 700px: stacked vertically (screenshot `14-two-pane-narrow.png`). Drag cross-pane guard: see §B. |

### G. Regression + hygiene
See G4 above (full evidence). Additionally noted, **out of scope for this audit**: at 700px viewport the global top nav bar (`FORGE`/`FLEET`/... / `STACKS`) visually overlaps — this is `+layout.svelte`'s site-wide header, untouched by PR #64 and present on every route, not a UI-2 regression.

### H. Edge cases / a11y
| Item | Verdict | Evidence |
|---|---|---|
| Empty stack → empty state, no crash | PASS | Screenshot `12-empty-stack.png`; "no loops yet / add one above." |
| Single card → no connector | PASS | Script: `connector count (expect 0): 0`. |
| Esc closes popover | PASS | Script: `popover count after Escape (expect 0): 0`. |
| One popover open at a time | PASS | Script: eval popover open (1) → opening schedule popover closes it (eval count 0, sched count 1). |
| Outside-click closes | PASS | Script: `popover count after outside click (expect 0): 0`. |
| Scroll closes | PASS, **re-verified after catching a false pass** | First attempt gave a false "still open" reading because the page wasn't tall enough to actually scroll (`wheel()` had nothing to do). Re-ran after padding the stack to 17 cards (`scrollHeight` 2903 vs `clientHeight` 1100) and confirmed `window.scrollY` actually moved (400px) before asserting the popover closed (count 0). Flagging the correction explicitly per the audit's own evidence rule — the first result would have been a bad PASS. |
| Bottom-sheet under 520px | PASS | 480px viewport: `.pop.sheet` present, full-width, bottom-anchored, rounded top corners. |

## 3. Fixes applied

**None.** No shipped-code defects were found in this audit — every finding was either (a) a coverage gap closed with a new test (§5), or (b) a design/policy question requiring Wes's decision (§4), per the brief's own rule that only trivial, obviously-correct defects get fixed in place and structural/ambiguous items get escalated instead.

## 4. Escalations

### 4.1 Budget badge reads as enforced when nothing enforces it
`StackConnector.svelte` renders `⏸ budget {value}` between cards whenever a card's `guardrails.budget !== 'auto'`, styled identically to the real (WIRED) schedule cadence badge, using a pause icon that connotes "this will stop something." Nothing server-side reads this value — confirmed absent from `cardToTaskPayload`'s output — so an operator could reasonably believe they've capped spend when they haven't. **Recommendation:** either (a) hide the badge entirely until a real budget field exists server-side, or (b) keep it but restyle distinctly from the WIRED schedule badge (e.g., dashed border + an explicit "(not enforced)" suffix, matching how the evals popover already avoids implying pass/fail). Not decided here per the brief's explicit instruction.

### 4.2 CI's own hard gates are largely soft-fail, repo-wide, pre-existing
11 `continue-on-error: true` instances in `.github/workflows/konjo-gate.yml`, present on `main` before PR #64 and unrelated to the UI-2 diff (which the workflow doesn't even touch — it's Rust-only, see G1/G5). Notably, the Wall-3 adversarial-review job's own "Fail if BLOCKER verdict" step has `continue-on-error: true`, meaning a genuine BLOCKER finding from the Konjo Adversarial Review would not currently fail CI at all. **Recommendation:** this is a repo-wide quality-framework policy decision (when to flip cargo audit/deny/coverage/rustdoc/Wall-3-blocker from advisory to hard-fail), not a UI-2-scoped fix — flagging for Wes rather than unilaterally hard-failing checks that may currently be failing for reasons unrelated to this audit.

### 4.3 Schedule-cron WIRED proof is weaker than the task-payload fields'
No `cardToSchedulePayload`-equivalent exists (see §2C). The narrower "the string format matches" claim is true and tested; the stronger "this object round-trips through a real API call" claim — which is what `cardToTaskPayload` proves for the other WIRED fields — isn't, and building it requires deciding what synthesizes a schedule's `name` (StackCard has none). **Recommendation:** decide the naming scheme (e.g., `card.alias ?? card.goal.slice(0, 40)`) as part of whichever backend sprint wires "Schedule stack" in `RunMenu`, then add the equivalent pure function + table test at that point — premature to build now against a naming convention nobody's chosen yet.

### 4.4 Playwright-in-deps check
**Result: clean.** No entry in `package.json` dependencies or devDependencies (see G5). The audit's own screenshot script used the environment's pre-installed system Playwright via a temporary symlink, never touching `package.json`; both the symlink and script were removed before finalizing this branch.

## 5. Coverage gaps closed this audit

`stack.test.ts` grew from 103 → **121** assertions (+18). All additions are either (a) closing a real gap the brief's checklist called out explicitly, or (b) correcting a test methodology flaw caught mid-audit:

1. **Cross-pane reorder isolation** (§2B) — 2 new tests proving `applyToPaneCards`-dispatched reorder/drag-reorder never touches another pane, including object-identity assertions.
2. **WIRED round-trip table** (§2C/G2) — a 9-row table-driven test (`stepMaxIterations` values 7 and 0, gate/until/on_fail/model/effort/repo) plus a key-completeness assertion. Also added a standalone "`until` off ⇒ omitted" test — the pre-existing tests only checked `gate`'s off-state, so a regression swapping the two fields could have slipped past.
3. **Custom cron never snaps to a matching preset** (§2C) — one test asserting `cronHuman` on a custom-flagged, daily-shaped raw cron still reads "custom cron."

**Methodology note on visual/interaction evidence:** all §2F/§2H visual and interaction claims were exercised against a live `npm run build && npm run preview` build using a throwaway Playwright script (`web/vv-shots.mjs`, plus two small standalone computed-style checks for the reduced-motion claim) — not committed, not added to `package.json`, deleted before finalizing this branch. `StackOutput`'s fed-a-taskId behavior (§2E) was demonstrated via a temporary, reverted `onMount` hook in `+page.svelte` gated behind a `?vvdemo=1` query param, driving the **real** `recordTranscript()` API with fixture `AgentEvent`s — never committed (confirmed via `git diff` showing zero changes to `+page.svelte` on this final branch).

---

*Auditor: Claude Sonnet 5, `chore/ui-2-vv` branch. See `NEXT.md` for the backend blockers this hands off to.*
