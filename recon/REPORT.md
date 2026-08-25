# Sprint U-0: Web Dashboard Recon Report

Read-only reconnaissance on the axum-served / SvelteKit web dashboard, emphasis
on the Loop Stacks page (`/stacks`) and its interactive controls. This
document reports evidence for a design decision the human will make; it does
not make that decision.

---

## 0. Pre-flight

**Parameters this run used** (v1 wasted a day defaulting to a stale `main`;
this run required explicit values, chosen after investigating the repo and
confirmed with the human before any capture work began):

```
RECON_REF        = 043ca18470de6a4ce49e626822abf4c590778fdb  (HEAD of
                    claude/dashboard-recon-sprint-u0-rfy9tl, forked directly
                    from origin/main post-merge of PR #181)
CANARY_SELECTORS = .pop.sched, .chipinput, .cfgrow
SCRATCH_URL      = http://localhost:5173 (Vite dev server, this container)
LOOP_STACKS_PATH = /stacks
```

`git rev-parse HEAD`: `043ca18470de6a4ce49e626822abf4c590778fdb`
`git status --short --branch`: clean, `## claude/dashboard-recon-sprint-u0-rfy9tl`,
only `recon/` and `tools/` untracked (this sprint's own output) at capture time.

**Canary resolution** (live DOM, after seeding a card and opening the
schedule + config popovers, see Step 2):

| selector | match count | verdict |
|---|---|---|
| `.pop.sched` | 1 | ✅ resolves |
| `.chipinput` | 4 | ✅ resolves |
| `.cfgrow` | 6 | ✅ resolves |

All three canaries resolved. **Proceeding.**

**20-commit dashboard log** (`git log -20 --date=short --pretty='%h %ad %an %s'
-- crates/lopi-ui/ web/src/ src/mcp_ui/ src/repl/`), newest first:

```
83dbd06 2026-07-30 Claude          merge: reconcile PR #181 (Loop Stacks xN/color fixes) with main
2b9cd72 2026-07-29 Claude          test: close the remaining G3 survivors from adding the EventSource seam
d986739 2026-07-29 Claude          test: close the 7 mutation-testing survivors G3 found on this PR's diff
b8c73f3 2026-07-29 Claude          fix(clippy): resolve pre-existing hard-lint violations found verifying workspace lints
f149113 2026-07-29 Claude          fix(resource-surface): bound both production unbounded channels
557c2ff 2026-07-29 Claude          feat(determinism): pin MSRV by bisection, workspace lints, overflow-checks
f79c319 2026-07-29 Claude          fix(security): name the whatsapp dev-mode signature bypass as an explicit override
f2c04f8 2026-07-28 Wesley Scholl   fix(web): xN loop-count grammar, facet colors, and running-card chrome
b93e68f 2026-07-28 Wesley Scholl   Merge pull request #180 from konjoai/claude/web-ui-running-prompt-x1guy9
70e3945 2026-07-28 Claude          feat(web): give the running-prompt view a Claude-Desktop-style restyle
165a771 2026-07-28 Wesley Scholl   fix(web): stack composer autocomplete, chip color, and popover bugs
929f9aa 2026-07-28 Claude          merge: reconcile Sprint E (economics layer) with main's demo/measurement sprint
154fefb 2026-07-28 Claude          fix(ci): rustfmt, doc-staleness re-verification, and mutation-testing gaps on PR #177
8d26835 2026-07-28 Claude          fix(economics): drills git-commit GPG hang, clippy field_reassign_with_default, missing test-module allows; bump to 0.37.0
55df00f 2026-07-28 Claude          feat(economics): Sprint E Part 5, CLI/web surfaces + committed-spend seeding
ca8e980 2026-07-28 Claude          merge: main into demo-measurement sprint, resolve conflicts with Sprint G
52cccf1 2026-07-28 Claude          feat(demo): web dashboard synthetic-data banner + README screenshots
5cafb4f 2026-07-28 Claude          feat(economics): Sprint E Part 1, Money, Pool, BudgetTier types + rate table staleness check
15c36b7 2026-07-28 Claude          feat(measurement): label every session-cost surface, add no-dispatch proof
7a43195 2026-07-28 Claude          refactor(web): split warm_up_state into web/warmup.rs
```

Newest dashboard commit (`83dbd06`) is dated 2026-07-30, today. `f2c04f8`
("xN loop-count grammar, facet colors, and running-card chrome") and
`165a771` ("stack composer autocomplete, chip color, and popover bugs"), the
exact PR #181 work this sprint's canaries target, are both present and
recent. **No staleness signal.**

---

## 1. Honest summary

The Loop Stacks dashboard is a client-heavy SvelteKit SPA with a real,
sizeable design-token system that the page mostly (but not entirely)
draws from. Functionally it's solid: composer, chip grammar, popovers, and
the run sequencer all work as documented, and this sprint found no dead
controls. The rough edges are concentrated in three places: a genuine WCAG
contrast failure on two disabled/low-emphasis text elements, a near-total
absence of visible keyboard-focus indicators on the toolbar row (80% of
sampled tab stops), and (this sprint's biggest finding) a *server-side*
decision (not a CSS one) that throws away incremental streaming granularity
before it ever reaches the browser, so "smooth text" is not purchasable with
animation work alone. Layout stability and animation count are both already
close to the sprint's own stated targets; noise, where it exists, is more a
keyboard-accessibility and contrast problem than a "too much motion" problem.

---

## 2. Rendering model and transport

**Rendering model: a SvelteKit SPA, not server-rendered templates and not
HTMX-style partials.** `web/` is a full SvelteKit 2 + Svelte 5 project
(`@sveltejs/adapter-static`, Vite 6, Tailwind). It is built (`npm run
build` → `vite build` → `web/dist/`) and the static output is **embedded into
the Rust binary at compile time** via `rust-embed`
(`crates/lopi-ui/src/web/static_assets.rs`):

```rust
#[derive(Embed)]
#[folder = "$CARGO_MANIFEST_DIR/../../web/dist"]
struct WebAssets;
```

`static_handler` serves the embedded build with SPA client-side-routing
fallback to `index.html`, and a bundled `placeholder.html` if `web/dist/` is
empty (pre-`npm run build`). In dev (this recon's `SCRATCH_URL`), Vite's own
dev server serves the same source directly with HMR.

**Live-update transport: WebSocket, one per session, at `/ws`.** The client
(`web/src/lib/stores/wsClient.ts`) opens exactly one `WebSocket` to
`${proto}://${location.host}/ws` on mount, with exponential-backoff
reconnect. `/sse` and `/ws/tasks` routes also exist server-side
(`crates/lopi-ui/src/web/mod.rs`) but the web dashboard's own client code
does not use them, only `/ws`. Every message is a JSON-encoded
`AgentEvent`/`SnapshotMessage` (`web/src/lib/types.ts`'s `WireMessage`),
applied through a single reducer (`web/src/lib/stores/agents.ts`).

**CSS location:** Tailwind utility classes plus per-component Svelte
`<style>` blocks (scoped by Svelte's own hashed-class mechanism, e.g.
`s-m9faYzKKr_H_` seen throughout `census_color.json`'s selectors) plus one
global sheet, `web/src/app.css`, which also hosts the token layer below.

**Design tokens, the important question.** lopi has two independent token
systems, and the web dashboard does **not** fully use the same one the
`lopi_get_stack_status` MCP widget uses:

- **MCP widget** (`src/mcp_ui/stack_status.html`): four tokens, `--fg`, `--bg`, `--muted`, `--dim`, switched via `@media
  (prefers-color-scheme: dark)`, rendered in the **system font stack**
  (`-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif`). Minimal,
  theme-aware, no brand colour.
- **Web dashboard** (`web/src/app.css` `:root`, mirrored in
  `web/tailwind.config.js`'s `theme.extend.colors.konjo`): **19
  `--konjo-*` custom properties**, `black`, `deep`, `paper`, `ice`,
  `ice-deep`, `ember`, `flame`, `jade`, `sun`, `rose`, `plasma`, `violet`,
  `violet-bright`, `mint`, `rose-muted`, plus `accent`/`accent-2` (dynamic,
  swapped per phase), no `prefers-color-scheme` support at all (dark-only),
  webfonts (Inter / JetBrains Mono, loaded from `fonts.googleapis.com`).
  Two more tokens (`--konjo-teal`, `--konjo-violet-light`) exist in
  `app.css` but are scoped to a Budget-page comment block, not global
  `:root` (see §9).

The two systems share **zero** token names or CSS custom-property scope.
The MCP widget's restraint (4 tokens, adaptive light/dark, system font) was a
deliberate, different design decision from the web dashboard's (19 tokens,
dark-only, branded webfonts). Whether that's intentional or accidental
drift is exactly the kind of call for §11.

**Build step:** `npm run build` (SvelteKit + Vite) is a separate step from
`cargo build`; the Rust build embeds whatever is already in `web/dist/` (or
ships the placeholder if that directory is empty). No CI-blocking coupling
was found between the two build steps in this pass.

**Font-loading note:** `fonts.googleapis.com` is unreachable from this
recon's sandboxed environment (`net::ERR_CONNECTION_RESET`, confirmed on
every capture). Every screenshot in this report renders in the system font
fallback stack, not the production Inter/JetBrains Mono webfonts, recorded
per the Step 3 determinism contract, and again in `LEDGER.md`.

---

## 3. Census A: colour and contrast

Full data: `recon/census_color.json`. Extracted from **computed** styles
(never source CSS) across 7 fixture states (S1, S4, S5, S6, S9, S12, S13) at
1440×900, including hover/focus-visible/disabled interaction states.

- **21 distinct rendered colours** observed across the covered states.
- **Token traceability: 9 of 21 (43%)** trace exactly to a `--konjo-*`
  custom property computed on `:root`; **12 of 21 (57%) are hardcoded**
  literals with no traceable token.
  - Two of those "hardcoded" colours are not actually novel: `#00ffd4`
    (`.gchip.alias`) and `#b79bff` (`.gchip.model`) exactly match
    `--konjo-teal` and `--konjo-violet-light` respectively, both defined in
    `app.css` but scoped to a Budget-page comment block rather than global
    `:root` (see §2). The value was copy-pasted rather than the token being
    promoted to global scope: a real (if narrow) "should be a token
    reference" finding.
- **Near-duplicate clusters** (Euclidean sRGB distance < 18):
  - **White cluster:** `#f5f5f5` (`--konjo-paper`, the token) vs. `#ffffff`
    (a hardcoded pure white): two whites, one traceable, one not.
  - **Near-black cluster, four members:** `#050505` (`--konjo-deep`),
    `#000000` (hardcoded pure black), `#0a0d0f` (hardcoded, faintly blue),
    `#0a0a0a` (`--konjo-black`). Two of the four ARE tokens; the other two
    are hardcoded near-duplicates of them. This is exactly the "six
    almost-identical greys" pattern the brief called out: here it's four
    near-blacks, half traceable and half not.
- **Contrast failures (WCAG AA, 4.5:1 body text)**, **2 found, both
  severe:**
  | selector | fg | bg | ratio | needs |
  |---|---|---|---|---|
  | `button.press.w-8` (an xN chain-repeat stepper button) | `#231000` | `#050505` | **1.11:1** | 4.5:1 |
  | `span.hrunlbl` (the dock's "running total" label) | `#231000` | `#0a0d0f` | **1.06:1** | 4.5:1 |
  Both use the same foreground, `#231000` (a near-black burnt-orange, likely
  intended as a subtle/de-emphasized glyph colour) against near-black
  backgrounds. A ratio of ~1.1:1 is not "low contrast": it is **functionally
  invisible**. This is the report's clearest concrete accessibility bug.
- **Interaction-state colours:** hover and focus-visible colour was
  identical to rest in the `chip-alias` sample (only background opacity
  shifted 0.02→0.03, a 1% difference), and the sampled `stack-controls`
  trigger's disabled state used `rgba(245,245,245,0.28)` (28% opacity, passes contrast comfortably against the dark chrome it sits on).
- **Simultaneous saturation on S13** (Loop Stacks page, populated, one
  1440×900 viewport): **12 distinct highly-saturated hues** rendered at
  once (`#231000, #ff9500, #00ffd4, #00d4ff, #b79bff, #ffcc00, #ff0066,
  #ffaacb, #ffaaaa, #00ff9d, #66b3ff, #ff4500`). That's the chip-token row's
  five colours (`:alias`/`@repo`/`;model`/`;effort`/`×N`) times two visible
  panes, plus status/badge colours, all live on screen together. This is
  the one number the brief asked for; no palette proposal follows from it
  here (see §10/§11).

No palette is proposed (per constraint).

---

## 4. Census B: streaming path

Full trace with code quotes: `recon/census_streaming.md`. Summary:

- **Hop 1 (invocation):** genuine incremental streaming, `--output-format stream-json --verbose --include-partial-messages`
  (`crates/lopi-agent/src/claude_spawn.rs`). Not buffered.
- **Hop 2 (stdout read):** line-buffered (`AsyncBufReader::lines()`), not
  `read_to_string`. Correct.
- **Hop 3 (event bus granularity: the finding):** `claude_events.rs`'s
  `parse_stream_event` explicitly discards every `content_block_delta`
  (`_ => Vec::new(), // block deltas are coalesced into the assistant
  line`). Only the **complete** `assistant` message produces a
  `StreamEvent::Text`/`Thinking`: one atomic chunk per finished block, no
  matter how long. `--include-partial-messages` is requested and paid for,
  then discarded.
- **Hop 4 (transport):** no batching: `event_bridge.rs` serializes and
  broadcasts each `AgentEvent` individually, immediately, over the
  WS/SSE-shared broadcast channel.
- **Hop 5 (client render):** append/extend, not full re-render: `transcript.ts`'s `appendText`/`appendThinking` concatenate onto the
  currently-open block; Svelte's keyed `{#each}` re-renders only the
  changed block.
- **Measurement:** the real, repo-checked-in capture
  (`artifacts/STREAM_CAPTURE.jsonl`, 44 lines / 3 turns) has **14**
  `content_block_delta` events collapse into **one** 126-character text
  mutation and one 81-character thinking mutation. Sample too small for a
  responsible p95 across a distribution (**not fabricated**), but the
  mechanism is proven with certainty: no incremental delta ever reaches the
  browser, and there is no upper bound on a single mutation's size.

**Verdict: hop 3, a `lopi-agent` change, not a `lopi-ui` CSS change, is what
blocks smooth streaming.** Per the standing instruction, this earns its own
sprint ahead of the visual work.

---

## 5. Census C: layout stability

Full data: `recon/census_layout.json`. Real `layout-shift`
`PerformanceObserver` entries, motion=on, 1440×900.

| scenario | total CLS | shift count |
|---|---|---|
| S4 streaming (log lines arriving) | **0.006** | 4 |
| S9 long scrollback (manual scroll mid-tail) | **0.0014** | 2 |
| S13 populated (open config popover) | **0.0014** | 2 |

All three are comfortably under Google's 0.1 "needs improvement" threshold. **Layout stability is not where this dashboard's problems live.** Named
shift sources (from `top_shifts`):

- `button.gutter` (the drag-to-resize pane splitter) widens from 6px→12px, almost certainly the resize-affordance widening on some trigger, not a
  bug in the usual sense.
- `div.hrun` (the dock's "running total" cost figure) shifts ~1px in x/width
  as its digit count changes, a classic "the number got wider" reflow,
  small (0.0005 CLS contribution) but real.

**Does anything above a new log line move?** Not measurably: CLS during
active streaming stayed at 0.006 across 4 shifts total, none of them the log
region itself.

**Does anything behind an opened popover move?** Not measurably at S13
(0.0014 CLS, 2 shifts, neither the popover's own components).

**Scroll anchoring / follow-tail check:** **inconclusive.** The generic
`[class*="log"], [class*="scroll"]` selector used to probe `scrollTop`
before/after a programmatic `mouse.wheel()` scroll returned `0` both times,
which likely means the wheel event did not land on the actual scrollable log
container rather than that the container genuinely never moved. Per the
brief's own instruction, this is reported as inconclusive rather than
guessed at.

---

## 6. Census D: animation inventory

Full data: `recon/census_animation.json`. Motion=on, 1440×900.

- **CSS `@keyframes` running during an active prompt:** **1**, a
  `spin`-named animation (1.1s linear infinite) on the running-card's status
  icon `<svg>`. Sampled 6× over 2s; count never exceeded 1.
- **Headline number: 1 concurrent CSS animation during a running prompt**. **This already meets Sprint U's own stated target** ("one, the spinner").
- **JS-driven timer, found separately (not caught by the CSS-only sampler
  above, but real and worth flagging alongside it):**
  `web/src/lib/stores/agents.ts`:
  ```ts
  elapsedTimer = setInterval(() => {
    agents.update((m) => {
      ...
      const elapsedMs = Date.now() - a.startedAt;
      const decayedActivity = Math.max(0, a.activity * 0.985);
      next.set(id, { ...a, elapsedMs, activity: decayedActivity });
  ```
  runs **every 250ms**, for **every running card simultaneously**. This is
  the exact "a number ticking every 250ms reads as motion to the eye even
  though it is not an animation" case the brief named as a hypothetical. It
  is real, not hypothetical, and its per-card multiplicity means N running
  cards means N-times-per-second store writes, independent of the "1
  spinner" CSS-animation count above. This is very likely a meaningful part
  of what reads as "busy" during a multi-agent run, and it is invisible to
  a CSS-only animation audit.
- **Things in motion on a single Loop Stacks row hover: 2**, `box-shadow`
  and `border-color` transitions, both 0.12s. No new `@keyframes` start on
  hover. This is a modest number. **The row-hover state is not where
  animation clutter concentrates**, contrary to the suspicion in the sprint
  brief's cover note; the 250ms elapsed-ticker above is a more likely
  source of "busy" perception than any single hover interaction.
- **`prefers-reduced-motion` respected: yes**, at least one stylesheet rule
  is guarded by a `prefers-reduced-motion` media query (confirmed via a live
  `document.styleSheets` walk, not a source-code grep).

---

## 7. Census E: component matrix

Full manifest: `recon/component_matrix.json`. State grids:
`recon/shots/components/{component_button|schedule_popover|config_popover|chip_token}_{1440x900|390x844}.png`.

**Scope note, stated plainly:** this sprint's time went first to the full
13-state × 5-viewport page sweep (Step 3, 55 real screenshots, S7/S8 are
correctly absent (see below) and Censuses A/B/C/D/F, which is where the
sprint's headline findings live. Census E below is a real, live-DOM-captured
but **narrower** slice of the brief's full state matrix (dropdown edge-flip
behavior, 30-chip overflow, alias-input suggestion lists, and destructive-
confirm steps were not exercised this pass), per the brief's own
permission to say so rather than photograph something adjacent and label it
as the state.

**Captured**, each as a labelled grid at two viewports (1440×900, 390×844):

| component | states captured | states not captured |
|---|---|---|
| config button ("add to stack") | rest, hover, focus-visible, disabled | active, pending, success, error, destructive-confirm |
| schedule popover | trigger rest/hover, open+positioned, open+grown (schedule toggled on, mounting the cron builder), dismissed by Escape | open near viewport edges, dismissed by outside-click/re-click |
| stack default config popover | trigger rest, open+positioned (6 `.cfgrow` rows, matching the canary count) | trigger hover/focus-visible, edge positioning |
| quick-insert chip token | rest, hover, focus-visible | selected, removable, mid-removal, disabled, 30-chip overflow, long-label |

**Dead controls:** none found in this pass. Every control exercised had an
observable effect (the schedule popover's toggle visibly mounted the cron
builder; the config popover's rows are live and match the canary count).
This is a narrower check than the brief's full sweep; absence of evidence
here is not strong evidence of absence project-wide.

**Destructive controls:** none exercised this pass mutated anything beyond
this recon's own scratch WebSocket/REST mocks (which are not real state to
begin with). No real backend was reachable from `SCRATCH_URL` regardless.

**Smallest hit areas / hover-only affordances:** not systematically measured
this pass (scope reduction above); the `button.gutter` pane-resize splitter
(6px wide at rest, seen in Census C) is the narrowest control noticed
incidentally and is a reasonable next-pass candidate for the 24×24 hit-area
check.

**A naming collision worth flagging while in this territory:** the client's
own `CardStatus` type calls one state `'blocked'`
(`web/src/lib/stores/stack.ts`), but its doc comment defines that as *"the
terminal state for a run that ended anything other than `completed`"* (i.e.
a finished, failed run with a reason), not "waiting to start." The brief's
S11 ("blocked task waiting on a dependency") was fixtured instead as a
`'queued'` card sitting behind a `'running'` card in the same chain: the
client's actual model of "waiting on a dependency." Two different concepts
share adjacent vocabulary in the codebase; worth a rename or a comment
clarifying the distinction (see §9).

---

## 8. Census F: keyboard, focus, and layering

Full data: `recon/census_keyboard.json`, `recon/census_interaction.json`.
25 Tab stops walked from a clean focus state on S13 (two populated panes),
1440×900.

- **Tab order matches visual order**: 0 out-of-order jumps detected across
  25 stops (checked: no backward x+y jump exceeding 40px in the "wrong"
  direction).
- **Focus indicator visibility, the headline finding: only 5 of 25 sampled
  tab stops (20%) have ANY visible focus indicator** (non-zero-width outline
  or a non-transparent box-shadow ring). The 5 that do: the composer
  (`.chipinput`, a proper dark-inset + accent-glow ring), the ×N chain-repeat
  `−`/`+` steppers, and a pane's `✕` close button. **Every single icon
  button in the stack-controls dock toolbar row has zero visible focus
  indication**: schedule, guardrails, evals, run-until-acceptance, default
  config, templates, duplicate, reorder, delete, plus every quick-insert
  chip token (`:alias`/`@repo`/`;model`/`;effort`/`×N`). These are
  keyboard-reachable (correct tab order) but **invisible when focused**.
  A keyboard-only user has no way to see where they are among 9+ toolbar
  actions.
  - Root cause, from the raw computed styles: these elements' `box-shadow`
    computes to `rgba(0,0,0,0) 0px 0px 0px 0px, rgba(0,0,0,0) 0px 0px 0px 0px`
    on focus (a fully transparent "ring"), while `outline` is separately
    forced to `none`/`0px`. The ring rule exists (it's the same mechanism
    the composer and steppers use successfully) but its colour resolves
    transparent for this element set.
- **Popover focus management, three gaps, all confirmed live:**
  1. **Focus does not move into the popover on open.** Clicking the
     schedule trigger leaves focus on the trigger button itself, not on any
     control inside the now-open `.pop.sched`.
  2. **Tab is not trapped inside an open popover.** From the trigger, one
     `Tab` press moved focus to a **sibling dock button** ("stack
     guardrails"), not to the popover's own first control (the "run on a
     schedule" toggle), while the popover was still open on screen.
  3. **Escape closes the popover (confirmed: `.pop.sched` count → 0) but
     does not return focus to the trigger.** Focus remained wherever Tab
     had left it ("stack guardrails"), not back on the button that opened
     the popover.
  Combined, these three mean: no visible focus ring, no focus trap, and no
  focus restoration. A keyboard-only user can open a popover, lose track
  of it, and land somewhere else in the toolbar with escape.
- **Keyboard-only reachability:** every control walked in the 25 Tab stops
  was reachable; no mouse-only affordance was found in this sample (a
  narrower check than the brief's full sweep; see §7's scope note).
- **Stacking (`z-index`) inventory:** **8 distinct explicit z-index values**
  in live use on S13: `2, 3, 10, 20, 30, 38, 39, 200` (full selector-level
  detail in `census_interaction.json`). No popover/dropdown clipping by an
  ancestor's `overflow` was observed in this pass's captures.

---

## 9. Trivially fixable things (deliberately not fixed)

- `web/src/lib/types.ts`'s `TaskStatus` union is missing `AwaitingPlanApproval`
  (present in `crates/lopi-core/src/task.rs`'s real enum). The client
  routes around this today with an explicit `as unknown as TaskStatus` cast
  in `wsClient.ts`'s demo generator rather than the type actually covering
  the wire contract. A one-line type addition, not touched.
- `--konjo-teal` / `--konjo-violet-light` exist in `app.css` but scoped to a
  Budget-page comment block instead of promoted to the same global `:root`
  as the other 17 tokens: `.gchip.alias`/`.gchip.model` on the Loop Stacks
  page use the identical hex values as hardcoded literals instead of
  inheriting the token. Not touched (moving a CSS custom property's scope
  is exactly the kind of "small" change this sprint's rule reserves for the
  human's call, since it touches shared token scope).
- The `'blocked'` CardStatus naming collision noted in §7 (means "finished
  in error," not "waiting to start"), not renamed.

No CSS/HTML/JS/Rust file serving the dashboard was modified. Both items
above are cited with file/line, not fixed.

---

## 10. Options, not recommendations

**Colour (§3).** Two thirds of rendered colours already trace to tokens for
the states sampled; the near-duplicate near-black cluster and the two
scoping-not-value token gaps are narrow, well-identified fixes, not a
palette overhaul.
  - *Option A: close the two Budget-token gaps and pick one near-black*
    (promote `--konjo-teal`/`--konjo-violet-light` to global `:root`; decide
    whether `#050505`/`#000000`/`#0a0d0f`/`#0a0a0a` should really be four
    values or two). Low effort, low risk, forecloses nothing.
  - *Option B: do nothing until Sprint U's other findings (contrast,
    focus) land first.* The contrast failure in §3 is the one item here
    that isn't a judgement call: it's a WCAG failure and probably should
    not wait for a palette decision either way.

**Contrast (§3), not a judgement call.** `#231000`-on-near-black at
~1.1:1 fails WCAG AA by a wide margin on two real controls. This should be
fixed regardless of what else Sprint U decides; it is flagged here, not
fixed, only because of this sprint's read-only rule.

**Streaming (§4).** Hop 3 is the blocker; hops 1/2/4/5 are already correct.
  - *Option A: thread partial `content_block_delta` text through
    `StreamEvent`/`AgentEvent` end-to-end.* Real plumbing work across
    `lopi-agent` → `lopi-ui` → `web`; the "big blocks" complaint disappears
    at the root. Higher effort, but this is the only option that actually
    fixes the complaint rather than working around it.
  - *Option B: client-side reveal animation (type-writer/fade-in) over the
    existing coarse chunks.* Cosmetic, cheap, ships fast, but the
    "smoothness" it buys is fake: a multi-thousand-character block still
    arrives atomically and gets typed out from a buffer, not streamed live.
    Worth doing only as a stopgap, not a replacement for Option A.
  - This is the one place in this report where "one option is clearly
    better" if the goal is what the brief's cover note asks for (genuinely
    smooth streaming). Option A is that option. Option B is a legitimate
    choice only if the team explicitly wants a cheap visual stopgap while
    Option A is scheduled separately.

**Animation (§6).** Already at or near the sprint's own stated target (1
concurrent CSS animation during a running prompt; 2 things on row hover).
  - *Option A: leave CSS/hover animation as-is; address the 250ms
    per-running-card store-write ticker instead* (throttle it, or decouple
    the visual "elapsed time" display from a full store re-derivation every
    250ms). This is where a multi-agent view's "busy" feeling most likely
    comes from, per this census.
  - *Option B: leave everything as-is.* Defensible given both measured
    numbers already meet target; the 250ms ticker's actual visual impact
    was not independently confirmed as user-perceptible "clutter" this pass
    (it's a code-level fact, not a subjective panel judgement).

**Keyboard focus (§8), not really a judgement call either.** 80% of
sampled tab stops have zero visible focus indication, and popovers neither
trap focus nor restore it on close. This is a correctness/accessibility gap
more than a design-taste question.
  - *Option A: fix the transparent-ring root cause for the toolbar/chip
    button class(es)* first (likely the highest-value, lowest-effort item
    in this whole report, one shared CSS rule, judging by how uniformly
    the transparency appears across 19 of 25 stops), then separately decide
    on focus-trap/restore for popovers (more involved, needs a small
    focus-management utility, probably shared across all 6+ popover
    components).
  - *Option B: defer focus-trap/restore, fix only the visible-ring gap
    now.* Lower effort, ships the highest-value 80% of this finding
    immediately; focus-trap is a real but separable piece of work.

---

## 11. Questions for the human

1. **The near-black cluster is four values, two of them tokens
   (`--konjo-deep` #050505, `--konjo-black` #0a0a0a) and two hardcoded
   (#000000 pure black, #0a0d0f faintly blue-black).** Is that genuinely
   four distinct surfaces that need to stay visually distinct, or should
   the two hardcoded ones collapse onto the nearest existing token? And
   Separately: do `--konjo-teal`/`--konjo-violet-light` deserve to be
   promoted to global `:root` scope now that the Loop Stacks page is
   already using their exact values as copy-pasted literals?
2. **Given hop 3 discards `content_block_delta` on purpose (not a bug that
   crept in: it's an explicit `_ => Vec::new()` with a comment explaining
   why), was that a deliberate simplicity tradeoff at the time, or does it
   need revisiting now that "smooth streaming" is an explicit Sprint U
   goal?** If it was deliberate, what was the original reasoning: log-line
   noise reduction? A simpler `StreamEvent` surface? Something else? And
   does that reasoning still hold given the goal has changed?
3. **The 250ms per-running-card elapsed-timer (`agents.ts`) is a real,
   quantified motion source this census's CSS-only animation count
   completely missed.** Is "one thing in motion" (the brief's stated target)
   meant to include JS-driven ticking counters, or only CSS `@keyframes`? If
   the former, this ticker needs to be part of Sprint U's scope explicitly,
   not just the spinner.
4. **19 of 25 sampled tab stops share one root cause (a transparent
   focus-ring colour) rather than being 19 independent gaps.** Is there a
   single shared CSS class/mixin behind the toolbar-icon-button and
   chip-token components where fixing the ring colour once would close
   most of this finding at once, or are these actually separate component
   implementations that each need their own fix? (This sprint didn't trace
   the shared-class hierarchy far enough to answer that itself.)
5. **Popover focus-trap/restore is missing across at least the schedule
   popover: is this true of all 6+ popover components** (config,
   guardrails, evals, goal, max, schedule), or does one of them already do
   this correctly and the schedule popover is the odd one out? Worth a
   quick pass across the rest before scoping a shared fix.

---

*Component state grids referenced above are embedded as image files under
`recon/shots/components/`; page-state screenshots under `recon/shots/pages/`;
the two 30-second videos and the S4 motion-on frame strip under
`recon/videos/` and `recon/shots/motion_frames_s4/` respectively. See
`LEDGER.md` for reproducibility details (fixture seed, tool versions,
environmental caveats).*
