# Sprint U-0 - Dashboard Recon Report

Read-only reconnaissance on the lopi web dashboard. No fixes applied. See `recon/LEDGER.md`
for exact tool versions, the fixture mechanism, and the determinism protocol; see
`tools/recon/` for the throwaway tooling that produced everything under `recon/`.

## 1. Summary

The dashboard is a real, live SvelteKit single-page app ("the Forge," now organized around
a "Loop Stacks" composer at `/stacks`) served by axum, talking to the backend over a
WebSocket (with SSE as a second, less-used transport). It is not a facade: task creation,
status, tool calls, and log lines are all real data driven by real `AgentEvent`s. The one
architectural surprise, and the fact that shaped this entire recon pass, is that **the
dashboard's primary views never read from the server's own history.** `panes`/`cards` -
the client-side state that actually puts a `StackCard` on screen - are pure in-memory
session state, seeded from nothing on load (`makeDefaultPanes()`: two empty panes, always),
and never rehydrated from `GET /api/tasks` or `GET /api/tasks/:id/logs`. Reopening the
dashboard does not show you what's running; it shows you two empty composers, no matter
how much history the store holds. That single fact drove the fixture design for this
sprint (see `recon/LEDGER.md`) and is itself one of this report's headline findings -
independent of anything about color or motion.

On the open technical question this sprint exists to answer: **lopi is genuinely
streaming.** The `claude` CLI is invoked with `--output-format stream-json
--include-partial-messages`, stdout is read line-by-line (not `read_to_string`), and every
decoded event is rebroadcast immediately with no batching at the event bus, the WS/SSE
transport, or the client. The "block-appearing text" complaint is real, but its root cause
is a single, precisely-located decision in lopi's own NDJSON parser (Census B, hop 3) -
not a buffering problem anywhere else in the pipeline, and not something that requires
replumbing lopi's transport before Sprint U can proceed.

## 2. Rendering model and transport

**Rendering model**: a JS app - SvelteKit (`web/`), statically built via
`@sveltejs/adapter-static` into `web/dist/`, embedded into the `lopi` binary at compile
time via `rust_embed` (`crates/lopi-ui/src/web/static_assets.rs`). Not server-rendered
templates, not HTMX partials. `cargo build` alone does *not* rebuild `web/dist/` - the
build script creates an empty directory so the Rust build succeeds, and the placeholder
page (`crates/lopi-ui/src/placeholder.html`) is served until someone runs `npm run build`
in `web/`. This recon ran that build first; check whether it's stale before trusting any
capture against a checked-out tree.

**Routes** (`crates/lopi-ui/src/web/mod.rs`'s `build_app`):

| Route | Method(s) | Serves |
|---|---|---|
| `/api/health` | GET | liveness |
| `/api/tasks`, `/api/tasks/:id` | GET/POST/DELETE | task CRUD (create refused in demo mode) |
| `/api/tasks/:id/plan/{approve,reject}` | POST | plan-approval gate |
| `/api/repos`, `/api/branches`, `/api/claude-commands` | GET | repo/branch/command discovery for the composer |
| `/api/agents/:id/checkpoint`, `/api/agents/:id/rate-limit` | GET/POST/DELETE | checkpoint + per-agent rate-limit registration |
| `/api/stats` | GET | fleet counts, daily cost/tokens |
| `/api/budget/breakdown`, `/api/economics` | GET | cost-by-model, 7-day trend, degradation tier (Sprint E) |
| `/api/spec` | GET | extracted spec surface |
| `/api/quality/trend` | GET | quality history |
| `/api/agents/:id/dag` | GET | agent DAG |
| `/api/tasks/:id/stream` | GET (SSE) | per-task event stream |
| `/api/tasks/:id/logs`, `/api/logs` | GET | historical log tail - **never called by `web/src`** |
| `/api/schedules`, `/api/schedule-chains` (+ sub-routes) | GET/POST/PUT/DELETE | cron scheduling + chains |
| `/api/quota` | GET | rate-limit window snapshots |
| `/api/maxx` (+ sub-routes) | GET/POST/PUT/DELETE | quota-headroom automation |
| `/api/loop-engineering` (+ sub-routes) | GET/POST | loop strategy/escalation, run traces |
| `/api/config`, `/api/version`, `/api/models` | GET | effective config, version, model catalog |
| `/api/ws-ticket` | POST | mints a ticket `/ws`/`/sse` accept in place of a Bearer header |
| `/metrics` | GET | Prometheus text |
| `/sse` | GET (SSE) | global event stream |
| `/ws`, `/ws/tasks` (legacy) | GET (WS upgrade) | global event stream, snapshot-on-connect |
| everything else | GET (fallback) | the SvelteKit SPA / static assets |

Every route above except the SPA fallback sits behind one `route_layer` stack: rate
limiting, then Bearer/ticket auth (`crates/lopi-ui/src/web/mod.rs`'s own doc comment notes
this was a deliberate post-S11 fix - `/sse`, `/ws`, `/metrics` used to sit *outside* both
layers).

**Transport for live updates**: WebSocket (`/ws`), sending a full `snapshot` on connect
(`build_snapshot`, reading `store.load_history(100)` plus per-task costs and status
counts) followed by every subsequent `AgentEvent` as its own JSON text frame, pre-serialized
once and fanned out to every subscriber (`event_bridge.rs`). SSE (`/sse`, and a
per-task-filtered `/api/tasks/:id/stream`) exists as a second transport carrying the same
events, but nothing in `web/src` was found to use it - the SvelteKit app is WS-only
(`wsClient.ts`). No polling interval anywhere in the live-update path.

**Historical log endpoints are dead code from the frontend's perspective.**
`GET /api/tasks/:id/logs` and `GET /api/logs` exist, are tested, and are wired into the
router - but a repo-wide search of `web/src` found zero call sites for either. The
Transcript/`StackOutput` panes are populated exclusively by live WS events accumulated
since the page connected, capped at `MAX_BLOCKS = 600` (`web/src/lib/stores/transcript.ts`).
Reload the tab mid-run and the transcript is empty again, even though the server still has
every line.

**CSS / design tokens**: two separate, non-overlapping systems.
- The MCP `lopi_get_stack_status` widget (`src/mcp_ui/stack_status.html`) defines its own
  tiny token set - `--fg`, `--bg`, `--muted`, `--dim` - switched via
  `@media (prefers-color-scheme: dark)`, i.e. it follows the OS/host theme, not lopi's own
  theme switcher.
- The web dashboard (`web/src/app.css`) defines a much larger "Konjo" token set -
  `--konjo-black/deep/paper/ice/ice-deep/ember/flame/jade/sun/rose`, an orb-state palette
  layered on top (`--konjo-plasma/violet/violet-bright/mint/rose-muted`), an `--konjo-accent`
  pair remapped per `[data-theme]`, plus a motion/elevation token set (`--ease-*`, `--dur-*`,
  `--glow-*`). Tailwind's `konjo.*` color scale (`web/tailwind.config.js`) largely mirrors
  the CSS variables but re-declares the hex values as separate literals rather than
  referencing the CSS custom properties, and adds its own `teal`/`violet-light`/`card`/
  `mist`/`veil` keys that exist only in Tailwind config, with no `--konjo-*` counterpart.

**The dashboard does not use the MCP widget's token set, and the MCP widget does not use
the dashboard's.** They are two independently-authored palettes that happen to share a
name ("lopi") and nothing else - worth an explicit decision either way, not an accident to
silently converge.

**Build step**: `web/` is a SvelteKit + Vite + Tailwind project (`npm run build` →
`@sveltejs/adapter-static` → `web/dist/`), completely separate from `cargo build`.

## 3. Census A - Colour and contrast

See `recon/census_color.json` for the full data this section summarizes.

{{CENSUS_A_SUMMARY}}

## 4. Census B - The streaming path

Full hop-by-hop trace with code quotes: `recon/census_streaming.md`. Headline: hops 1, 2,
4, and 5 are all fine-grained and unbuffered; the coarseness is introduced once, on
purpose, at hop 3 - lopi's parser explicitly discards the CLI's incremental
`content_block_delta` events (the ones `--include-partial-messages` exists to request) and
reconstructs assistant text only from the final, complete `assistant` line.

{{CENSUS_B_SUMMARY}}

## 5. Census C - Layout stability

See `recon/census_layout.json`.

{{CENSUS_C_SUMMARY}}

## 6. Census D - Animation inventory

See `recon/census_animation.json`.

{{CENSUS_D_SUMMARY}}

## 7. Trivially fixable things deliberately not fixed

- `web/src/app.css`'s Tailwind `konjo.*` color scale re-declares the same hex values
  `app.css`'s `--konjo-*` custom properties already hold, instead of referencing
  `rgb(var(--konjo-accent-rgb) / <alpha-value>)`-style indirection the way `konjo.accent`
  already does. A one-character-class drift between the two lists (see Census A's clusters)
  is exactly the kind of thing this duplication makes possible. Noted, not touched.
- `recon/census_streaming.md`'s hop-3 finding (`content_block_delta` events discarded in
  `parse_stream_event`) reads like a one-line fix (route them to a new `StreamEvent::TextDelta`
  and append instead of replace) - but it is a real behavior change to a fuzzed, tested
  parser with a corpus seeded from a real capture, and it is the actual subject of the
  Sprint U scoping question below, not a drive-by patch.
- `web/src/lib/stores/agentReducer.ts`'s `VerifierVerdict` handling stores `fix_hints` on
  `AgentState` but nothing in `web/src` ever reads it back out for display - only
  `gaps[0]` ever reaches a human, everywhere it's shown (`transcript.ts`, `events.ts`,
  `stackRun.ts`'s `blockReasonFor`). Not fixed here: which of several existing consumers
  should grow a `fix_hints` display, and how, is a design call, not a typo.
- `GET /api/tasks/:id/logs` / `GET /api/logs` have zero callers in `web/src` (grepped). Dead
  from the frontend's perspective - not touched, since removing a tested, working endpoint
  is not a "trivial" call either way.

## 8. Options, not recommendations

**Colour and contrast.**
1. Collapse the near-duplicate clusters Census A found into fewer named tokens, keep
   everything else. Low effort, low risk, forecloses nothing - but only fixes the clusters
   this run's fixed set of states happened to expose.
2. Route the Tailwind `konjo.*` scale through the `--konjo-*` CSS custom properties instead
   of re-declaring hex literals, so the two lists can't drift again structurally. Medium
   effort (touches every Tailwind class site indirectly through the config, not the
   markup), but it's a one-time fix to the *class* of bug, not just this instance of it.
3. Leave the palette as-is and address only the contrast failures Census A flagged. Lowest
   effort, but the "noise" complaint (ranked #1) is explicitly about volume/near-duplication,
   not just accessibility, so this alone likely doesn't resolve what was actually reported.

**Streaming.**
1. Wire `content_block_delta` into a new incremental `StreamEvent` variant, append instead
   of replace on the client. This is the one place where I think the choice is closer to
   clear-cut than a genuine toss-up: hops 1/2/4/5 already support it end to end, the fix is
   scoped to `claude_events.rs` + `transcript.ts`'s append path, and it directly answers
   "why does text arrive in blocks." Real work: the existing fuzz corpus and parser tests
   assume the current coalesced shape and need updating alongside it.
2. Leave text at message-block granularity, but shrink the visual "chunkiness" by animating
   each new block in with a soft fade/slide instead of a hard append. Cheaper, ships in
   Sprint U proper, but treats the symptom (visual arrival) rather than the actual cause
   (block-level granularity) - the human specifically wants to know which this is, and this
   option is the "cosmetic" one.
3. Do both, in sequence - hop-3 fix first (own PR, own test updates, its own scope per the
   brief's closing warning about this being a plumbing change, not a polish task), soft-
   arrival animation as a Sprint U polish item once real deltas exist to animate.

**Layout stability.** Options depend entirely on what Census C's CLS numbers show once
captured - see section 5's data before deciding. If mid-stream shifts are real: fixed-
height/`overflow-anchor` containment on the live-output scroller is the standard, low-risk
fix; if they're not, no action needed and this section of Sprint U can be skipped entirely.

**Animation.** If Census D's concurrent-motion count is above 1 during a running prompt:
1. Cut everything but the spinner during active streaming specifically (matches the
   stated Sprint U target exactly) - most invasive to existing CSS, clearest outcome.
2. Leave ambient animations (void-drift, shimmer) running only when *nothing* is streaming,
   suppress them the instant a task goes live. Splits the difference - ambient motion is
   arguably fine at rest, the complaint is about *simultaneous* motion while working.
3. Leave as-is if the number turns out to already be low (see Census D) - no forced choice
   needed if the data doesn't support one.

## 9. Questions for the human

- Census A's near-duplicate clusters (numbers filled in from `census_color.json`): for each
  cluster, is it one token or several with a real distinction (e.g. running vs idle needing
  its own hue), or is it a fixable drift? A per-cluster call, not a single "reduce the
  palette?" yes/no.
- Is `--konjo-*` (dashboard) vs `--fg/--bg/--muted/--dim` (MCP widget) an intentional split
  - the MCP widget deliberately following host OS theme rather than lopi's own theme
  switcher - or should the widget adopt the Konjo tokens too? This wasn't a question until
  this recon pass found the two systems don't share a single value.
- The hop-3 streaming finding: is fixing `content_block_delta` handling in scope for Sprint
  U at all, or does it need its own sprint ahead of the visual work, per the brief's own
  closing expectation? The code-level fix is small; the test/fuzz-corpus blast radius and
  "is this actually what real Claude Code sessions look like at the block-size Census B
  measured" are the open questions, not the mechanism.
- `fix_hints` are computed, stored, and never shown. Worth surfacing at all, and if so,
  where - inline in the gate-failure card, in a dedicated panel, only on hover?
- Given panes/cards never rehydrate from the server: is that an accepted, permanent design
  (session-scoped composer, full stop), or is "show me what's still running when I reopen
  the tab" an actual product gap this recon incidentally surfaced? Neither Sprint U nor this
  recon pass was scoped to answer that, but it changes what "the dashboard" even means for
  every other question in this report.
