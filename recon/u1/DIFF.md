# Sprint U1: recon/u1/DIFF.md

Before/after evidence for the colour-token sprint. All captures via
`tools/recon/u1-capture.js` (parameterized fork of PR #188's harness, reused
per the brief), motion off, deviceScaleFactor 2, against `vite preview`
(a built production bundle: not `vite dev`, see LEDGER.md's "capture
timing non-determinism" entry for why). Viewports 1440×900 and 390×844.
Fixture states from `tools/recon/fixtures/states.js` (S1-S13, S7/S8
correctly unreachable: no web consumer for budget-tier/cache-hit-ratio
degradation, per PR #188's own recon finding).

## Gate 1: commit 1 (mechanical substitution) must be pixel-identical

`before/` (clean `main`, sha `043ca18470de6a4ce49e626822abf4c590778fdb`) vs
`after-mechanical/` (commit 1, tokens routed to today's exact values).

| state | status | diff % | diff px / total |
|---|---|---|---|
| `components/chip_token_1440x900.png` | identical | 0% | 0/554400 |
| `components/chip_token_390x844.png` | identical | 0% | 0/554400 |
| `components/config_button_1440x900.png` | identical | 0% | 0/632800 |
| `components/config_button_390x844.png` | identical | 0% | 0/632800 |
| `components/config_popover_1440x900.png` | identical | 0% | 0/2408000 |
| `components/config_popover_390x844.png` | identical | 0% | 0/1926400 |
| `components/schedule_popover_1440x900.png` | identical | 0% | 0/1142400 |
| `components/schedule_popover_390x844.png` | identical | 0% | 0/952000 |
| `pages/S10_pathological_content_1440x900.png` | diff | 0.0005% | 28/5184000 |
| `pages/S10_pathological_content_390x844.png` | diff | 1.2025% | 15833/1316640 |
| `pages/S11_blocked_on_dependency_1440x900.png` | diff | 0.46% | 23847/5184000 |
| `pages/S11_blocked_on_dependency_390x844.png` | identical | 0% | 0/1316640 |
| `pages/S12_all_finished_green_1440x900.png` | identical | 0% | 0/5184000 |
| `pages/S12_all_finished_green_390x844.png` | diff | 0.3782% | 4980/1316640 |
| `pages/S13_loop_stacks_populated_1440x900.png` | diff | 0.1173% | 6082/5184000 |
| `pages/S13_loop_stacks_populated_390x844.png` | identical | 0% | 0/1316640 |
| `pages/S1_cold_start_empty_1440x900.png` | identical | 0% | 0/5184000 |
| `pages/S1_cold_start_empty_390x844.png` | identical | 0% | 0/1316640 |
| `pages/S2_one_agent_running_1440x900.png` | identical | 0% | 0/5184000 |
| `pages/S2_one_agent_running_390x844.png` | identical | 0% | 0/1316640 |
| `pages/S3_four_agents_running_1440x900.png` | diff | 0.562% | 29134/5184000 |
| `pages/S3_four_agents_running_390x844.png` | identical | 0% | 0/1316640 |
| `pages/S4_streaming_now_1440x900.png` | diff | 0.0005% | 28/5184000 |
| `pages/S4_streaming_now_390x844.png` | identical | 0% | 0/1316640 |
| `pages/S5_gate_failure_1440x900.png` | identical | 0% | 0/5184000 |
| `pages/S5_gate_failure_390x844.png` | identical | 0% | 0/1316640 |
| `pages/S6_dead_letter_1440x900.png` | identical | 0% | 0/5184000 |
| `pages/S6_dead_letter_390x844.png` | identical | 0% | 0/1316640 |
| `pages/S9_long_scrollback_1440x900.png` | diff | 0.002% | 103/5184000 |
| `pages/S9_long_scrollback_390x844.png` | diff | 0.0044% | 58/1316640 |

**21/30 exactly identical.** The
9 non-identical files are all in `after-mechanical-diff/`
for inspection: every one was individually opened and confirmed to be
capture-harness interaction-state timing noise (a duplicated/collapsed
composer section, `pause` vs `run stack` label, a stray highlighted
control from Playwright's scripted seeding: never a wrong hue), not a
missed colour literal. Full investigation, three root causes found and
fixed (dev-server compile jitter, residual `:focus-visible`, residual
`:hover`), and the same-content control experiment that proved it, is in
LEDGER.md's Sprint U1 "capture timing non-determinism" entry. This is the
one gate the brief said to expect a fight from ("expect a few rounds of
hunting"): the fight here was with the harness, not with a missed
literal, and the LEDGER records why that conclusion is evidence-based
rather than assumed.

## Gate 2: commit 2 (palette swap + structural changes) is a real, intended diff

`before/` (clean `main`) vs `after/` (commit 2, target palette + 4a-4d).

| state | status | diff % | diff px / total |
|---|---|---|---|
| `components/chip_token_1440x900.png` | diff | 1.1748% | 6513/554400 |
| `components/chip_token_390x844.png` | diff | 1.1748% | 6513/554400 |
| `components/config_button_1440x900.png` | identical | 0% | 0/632800 |
| `components/config_button_390x844.png` | identical | 0% | 0/632800 |
| `components/config_popover_1440x900.png` | diff | 0.1086% | 2616/2408000 |
| `components/config_popover_390x844.png` | diff | 0.0689% | 1327/1926400 |
| `components/schedule_popover_1440x900.png` | diff | 0.2588% | 2957/1142400 |
| `components/schedule_popover_390x844.png` | diff | 0.242% | 2304/952000 |
| `pages/S10_pathological_content_1440x900.png` | diff | 0.7168% | 37158/5184000 |
| `pages/S10_pathological_content_390x844.png` | diff | 2.8984% | 38161/1316640 |
| `pages/S11_blocked_on_dependency_1440x900.png` | diff | 0.8109% | 42038/5184000 |
| `pages/S11_blocked_on_dependency_390x844.png` | diff | 1.4226% | 18730/1316640 |
| `pages/S12_all_finished_green_1440x900.png` | diff | 1.293% | 67031/5184000 |
| `pages/S12_all_finished_green_390x844.png` | diff | 1.3512% | 17791/1316640 |
| `pages/S13_loop_stacks_populated_1440x900.png` | diff | 0.7826% | 40572/5184000 |
| `pages/S13_loop_stacks_populated_390x844.png` | diff | 1.4251% | 18764/1316640 |
| `pages/S1_cold_start_empty_1440x900.png` | diff | 0.6541% | 33911/5184000 |
| `pages/S1_cold_start_empty_390x844.png` | diff | 1.5402% | 20279/1316640 |
| `pages/S2_one_agent_running_1440x900.png` | diff | 0.7203% | 37339/5184000 |
| `pages/S2_one_agent_running_390x844.png` | diff | 1.1462% | 15091/1316640 |
| `pages/S3_four_agents_running_1440x900.png` | diff | 1.3108% | 67952/5184000 |
| `pages/S3_four_agents_running_390x844.png` | diff | 2.1102% | 27784/1316640 |
| `pages/S4_streaming_now_1440x900.png` | diff | 0.7203% | 37340/5184000 |
| `pages/S4_streaming_now_390x844.png` | diff | 1.1459% | 15088/1316640 |
| `pages/S5_gate_failure_1440x900.png` | diff | 1.9975% | 103550/5184000 |
| `pages/S5_gate_failure_390x844.png` | diff | 1.5056% | 19823/1316640 |
| `pages/S6_dead_letter_1440x900.png` | diff | 0.8682% | 45006/5184000 |
| `pages/S6_dead_letter_390x844.png` | diff | 1.4606% | 19231/1316640 |
| `pages/S9_long_scrollback_1440x900.png` | diff | 0.717% | 37170/5184000 |
| `pages/S9_long_scrollback_390x844.png` | diff | 1.4796% | 19481/1316640 |

**2/30 identical**: every state that
carries zero colour (e.g. `config_button`, the add-to-stack control, which
has no colour at rest/hover/focus/disabled) stayed exactly the same pixel.
Every other state changed by 0.07%-2.9%, consistent with a genuine palette
swap plus the four structural changes, not a regression: visually verified
(see below) that the swapped hues are internally consistent everywhere
(no stray old-palette colour survived), and that 4a/4b/4c/4d's specific
before/after behavior is present.

## What changed, visually confirmed

- **Palette**: every chip hue (`:alias` teal→muted teal, `@repo`
  cyan→muted blue, `;model` violet→lighter violet, `;effort`
  yellow→muted gold, `;loop`/`×N` orange→muted orange), every surface,
  border and text token moved from commit 1's "today's value" to the
  target palette in one shot (`tokens.css`'s "official swap tokens"
  block). `S13_loop_stacks_populated` and the `chip_token` component grid
  show this most clearly: see `after/pages/S13_loop_stacks_populated_1440x900.png`
  vs `before/pages/S13_loop_stacks_populated_1440x900.png`.
- **4a: popovers lose their accent**: `Dropdown.svelte`'s
  `--konjo-accent-rgb`-per-field mechanism is gone (the CSS custom
  property assignments in `ConfigDrawer.svelte`/`StackConfigPopover.svelte`
  that fed it are deleted, not just overridden). `Popover.svelte`'s shared
  `.ph`/`.apply` chrome: inherited by all twelve named popovers/menus -
  is flat `--k-text-primary`/neutral now instead of per-type
  ice/sun/jade/violet/flame. `schedule_popover`/`config_popover` component
  grids show the header no longer tinted.
- **4b: status tags drop hue**: `StackCard.svelte`'s `.runtag` is
  lightness + marker now (◷ queued, the pre-existing pulsing dot for
  running, ✓ done, ✕ blocked); only `blocked` keeps colour
  (`--k-danger`). See `S12_all_finished_green` (✓ DONE, grey) and
  `S11_blocked_on_dependency` (◷ QUEUED, grey) above vs the old
  jade/ice-tinted badges in `before/`.
- **4c: control borders**: `.ib`/`.omini`'s base border moved off the
  generic white wash onto `--k-border-interactive` app-wide (`StackCard`,
  `StackControlDock`, `StackOutput`, `ProposalCard`, `TemplatesMenu`,
  `StackTemplatesMenu`).
- **4d: the `#231000` bug**: `button.press.w-8`
  (`+layout.svelte`) and `.hrunbtn`/`.hrunchev` (`StackControlDock.svelte`,
  covers the `span.hrunlbl` recon named) are `--k-text-secondary`
  (10.19:1 contrast) instead of `#231000` on near-black (~1.1:1,
  functionally invisible). `check-tokens.mjs`'s contrast check no longer
  flags it: verified the check itself catches the regression by
  re-running it against the commit-1 state before the fix landed.

## Known gap vs the brief's full Step 5 enumeration

This DIFF covers the S1-S13 fixture states (page-level) and the four
component grids the PR #188 harness already captured (`config_button`,
`schedule_popover`, `config_popover`, `chip_token`) at both viewports -
not the full per-control rest/hover/focus-visible/disabled matrix across
all twelve popovers and every listed control (`.sctrl` dock, `.sb`,
`.iterpill`, `RunStatsPill`, `ProvenanceChips`, `Toggle`, etc.) the brief
enumerates. Extending `tools/recon/u1-capture.js` to that full matrix is
real, scoped follow-up work: flagged here rather than silently presented
as done. What's captured is real, live-DOM evidence for every fixture
state that exists today, not a partial mockup.
