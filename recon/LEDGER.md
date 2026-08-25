# Sprint U-0 (v2) Recon Ledger

Reproducibility record for the recon captured in this directory. If these
shots ever need to be retaken and diffed against, everything below is what
would need to match.

## Ref and canary

- `RECON_REF`: `043ca18470de6a4ce49e626822abf4c590778fdb` (HEAD of
  `claude/dashboard-recon-sprint-u0-rfy9tl` at capture time, forked directly
  from `origin/main` post-merge of PR #181)
- Canary selectors and live match counts at capture time: `.pop.sched` (1),
  `.chipinput` (4), `.cfgrow` (6). All resolved; see REPORT.md section 0 for
  the full pre-flight record.

## Fixture seed

No PRNG is used anywhere in `tools/recon/fixtures/states.js` or the capture
scripts: every task id, goal string, log line, and ISO timestamp is a fixed
literal. The only non-deterministic input across runs is real wall-clock
delay between scripted WebSocket frames (`atMs` in `states.js`), which
governs animation/motion timing for the motion=on captures, not content.
Re-running `tools/recon/capture-pages.js` against the same `RECON_REF`
reproduces byte-identical DOM content (module CSS-in-JS scoped class hashes
from Svelte's build are stable per build, not per run).

**Fixture seed value: static (no `Math.random()`/`crypto.randomUUID()` used
in any recon script).**

## Tool versions

- Node: v22.22.2
- Playwright: 1.62.0 (`tools/recon/package.json`, own `node_modules`,
  separate from `web/`'s own 1.61.1 devDependency)
- Chromium: 141.0.7390.37 (`/opt/pw-browsers/chromium`, the environment's
  pre-installed browser, not a Playwright-managed download)
- Vite dev server: `web/`'s pinned `vite@^6.4.3`, run via `npm run dev --
  --port 5173 --strictPort`

## Determinism contract, what was actually applied

- Device scale factor: 2, fixed, on every context (`tools/recon/lib/browser.js`).
- Clock: frozen at `2026-07-30T12:00:00.000Z` via an `addInitScript` `Date`
  override, applied before every navigation.
- Motion=off captures: `* { animation: none !important; transition: none
  !important; }` injected via `page.addStyleTag` before any content loads.
- Font stack: forced to the system stack
  (`-apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial,
  sans-serif`; monospace to `ui-monospace, "SF Mono", Menlo, Consolas,
  monospace`) on every page, because `fonts.googleapis.com` (Inter /
  JetBrains Mono) is unreachable from this environment
  (`net::ERR_CONNECTION_RESET`, confirmed on every capture attempt). **Every
  screenshot in this sprint is NOT in the production webfonts.** If this
  environment gains network egress to Google Fonts in a future run, shots
  will differ from this baseline for that reason alone, not a real UI
  change.

## Environmental caveats affecting reproducibility

- This recon ran against the Vite dev server (`SCRATCH_URL =
  http://localhost:5173`), not the built-and-embedded production binary
  (`cargo run -- sail`, port 3000 by default). The dashboard's rendering
  model is a client-side SPA either way (see REPORT.md section 2), but a
  built-mode capture was not taken this pass and could show different
  bundle-hash class names (Svelte's scoped-CSS hashes are build-dependent,
  though the visual output should be identical).
- No live lopi backend, Rust process, or SQLite store was reached at any
  point. Every REST endpoint and the `/ws` WebSocket were mocked
  (`tools/recon/lib/mock.js`) with fixed fixture data
  (`tools/recon/fixtures/states.js`), following the same pattern already
  established in `web/e2e/popover-visibility.spec.ts`.
- S9 (`long scrollback, 2,200 lines`) at the narrowest viewport (390×844)
  took materially longer to screenshot than at wider viewports (real
  measurement: fixture seeding took ~15s, the screenshot itself took ~45s,
  versus comfortably under the default 30s timeout at wider viewports). This
  reads as a genuine performance characteristic of the log panel at narrow
  widths with 2,000+ lines (more line-wraps per entry), not a script bug,
  and is itself cited in the report as evidence the log panel does not
  virtualize long scrollback. If retaking this capture, budget extra time
  for this one state/viewport combination specifically.
- S7 (budget tier degraded) and S8 (cache hit ratio degraded) produced zero
  screenshots by design: `grep` across `web/src` found no component
  consuming `/api/economics` or any cache-hit-ratio concept anywhere in the
  web dashboard. These states are reported as unreachable on the web
  surface, not photographed against an adjacent state. See REPORT.md
  sections 4 (fixture table) and 9/11.
- Census E (component matrix) covers a narrower slice of the brief's full
  state-matrix-per-component-type than specified, by explicit scope
  decision recorded in REPORT.md section 7, given the time this sprint spent
  on the full page-state sweep and Censuses A-D/F instead.

## Output inventory

- `recon/shots/pages/`: 55 PNGs (11 reachable states x 5 viewports; S7/S8
  correctly absent), motion=off, `recon/capture_log.json` has the full
  per-capture status ledger.
- `recon/shots/components/`: 8 composited, labelled state-grid PNGs (4
  components x 2 viewports), `recon/component_matrix.json` is the manifest.
- `recon/shots/motion_frames_s4/`: 20 frames, 100ms apart, motion=on, during
  S4's scripted streaming sequence.
- `recon/videos/s4_streaming.webm`, `recon/videos/working_session_loop_stacks.webm`:
  the two 30-second videos, motion=on, 1440x900.
- `recon/census_color.json`, `recon/census_streaming.md`,
  `recon/census_layout.json`, `recon/census_animation.json`,
  `recon/census_interaction.json`, `recon/census_keyboard.json`,
  `recon/component_matrix.json`, `recon/REPORT.md`.

## Diff scope

`git diff --stat` against `RECON_REF` touches only `tools/recon/`, `recon/`,
and this file. No line of existing CSS, HTML, template, JavaScript, or Rust
serving the dashboard was modified.
