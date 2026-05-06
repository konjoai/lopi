# lopi UI — The Forge
## A Living Window into Agent Cognition

> *Make it Konjo: beautiful, lean, immersive, alive.*

---

## The Vision

The current `lopi-ui` is a working dashboard. Functional. Invisible to anyone who isn't already invested in agent orchestration. **This document defines what comes next:** a UI that turns heads, draws people in, and makes the act of running an AI agent feel like watching a star ignite.

The centerpiece is **the Forge** — a perpetually-morphing sphere of fire and ice that breathes, pulses, and reacts to the live cognitive state of every agent in flight. Around it, surgical data: phase wheels, thought streams, decision lattices, cost counters. All driven by the existing WebSocket. All rendered at 60fps.

This is not a dashboard. It is a **planetarium** for agent thought.

---

## Design Principles

1. **One focal point.** The Forge dominates the canvas. Everything else orbits it.
2. **Show, don't list.** Token pressure isn't a number — it's the density of fire in the sphere. Phase isn't a label — it's the color of the aurora around the core.
3. **Built for non-developers too.** A founder watching their team should understand what's happening at a glance. A child should think it looks cool.
4. **No chrome. No clutter.** The UI is an instrument, not an application. Every pixel earns its place.
5. **Konjo aesthetic.** Pure black, ember orange, ice cyan, single accent. Mono type for code, geometric sans for labels. No gradients except where physics demands.

---

## Architecture

```
lopi (rust)                          web/ (this directory)
┌──────────────────┐                 ┌────────────────────────┐
│  AgentRunner     │  WebSocket      │  SvelteKit + Three.js  │
│  EventBus        │ ──/ws/tasks──▶  │  app                   │
│  TurnMetrics     │  TaskStatus,    │                        │
│  ContextWindow   │  AgentEvent,    │  Renders:              │
│                  │  TurnMetrics    │   • The Forge (WebGL)  │
│  axum:3000       │ ◀──/api/tasks── │   • Phase wheel        │
└──────────────────┘                 │   • Thought stream     │
                                     │   • Constellation      │
                                     └────────────────────────┘
                                              │
                                       Built static, embedded
                                       in lopi binary via
                                       rust-embed for shipping.
```

**Stack**
- **SvelteKit 2** — fewer ceremony, less JavaScript, faster cold load than React
- **Three.js** — the standard for in-browser 3D; raw GLSL shaders for the Forge
- **TailwindCSS** — utility classes for layout; custom CSS variables for the palette
- **TypeScript** — types for WebSocket messages mirror lopi-core Rust types

**Why not Next.js?** SvelteKit ships less runtime, has cleaner reactive state, and renders the Forge with less re-render overhead. The Konjo answer is the leaner answer.

---

## The Forge — Killer Feature

A 3D sphere rendered with custom GLSL. The fragment shader composes three layers:

### Layer 1 — Volumetric Noise
Three octaves of simplex noise (3.0, 8.0, 16.0 frequency) blended at (0.6, 0.3, 0.1). The surface displaces with the noise; the surface coloring is keyed by the noise value.

### Layer 2 — Fire / Ice Domains
A sinusoidal boundary modulated by the noise field divides each fragment into a fire domain (warm, ember orange) or an ice domain (cool, cyan). The boundary swirls with time. Hot spots emerge from high-frequency noise peaks; cold veins from low-frequency troughs.

### Layer 3 — Fresnel Aura
A view-direction Fresnel term (`pow(1 - dot(view, normal), 2)`) drives an outer glow tinted by the **phase color** — the agent's current phase (Boot/Discovery/Planning/Implementation/Testing/Conclusion). The aura is the agent's emotional state.

### Live Inputs from lopi
| Shader uniform | Source | Effect |
|---------------|--------|--------|
| `uTime` | RAF clock | continuous animation |
| `uPressure` | `ContextWindow.token_pressure()` | turbulence + displacement intensity |
| `uPhaseColor` | `Phase` enum from runner | aura tint, accent |
| `uActivity` | tokens/sec from TurnMetrics | pulse rate |
| `uHealth` | success rate from MemoryStore | overall warmth |

When the agent is calm and reading, the sphere is mostly ice. When it's generating heavily, fire dominates and the surface roils. When it transitions phases, the aura shifts color across the spectrum. When eviction fires, ripples propagate outward.

---

## Supporting Visualizations

### Phase Wheel
A circular SVG indicator at top-right. Six segments: Boot, Discovery, Planning, Implementation, Testing, Conclusion. The active segment glows in the phase color; completed segments retain a faint trail. Animated as the agent moves through its lifecycle.

### Thought Stream
A scrolling log of the agent's planning text — but rendered as a **flowing river of glyphs** with new tokens fading in at the bottom and older ones drifting up and out. Mono font. Letterform-by-letterform animation. Looks like watching thoughts coalesce.

### Token Gauge
A vertical bar on the right edge representing the current context fill. Fills from cool blue (low pressure) to hot orange (high pressure). The eviction threshold (75%) is a visible bright line. Eviction events pulse the bar.

### Constellation
The "all agents" view. Each running agent is an orbiting body around an empty center. Distance from center = priority. Size = context pressure. Trail length = recent activity. Click any body to zoom into its Forge view.

### Cost Counter
Bottom-right. The current run's accumulated cost. Counts up character-by-character (like a slot reel) when API calls land. Color shifts toward red as it approaches the circuit breaker cap.

### Log Terminal
Bottom-left. JetBrains Mono. Color-coded by level (info=ice, warn=ember, error=red). Auto-scrolls but pauses on hover. Per-task ID prefix. Click to filter.

---

## Color Palette (The Konjo Theme)

```css
:root {
  --konjo-black:    #0a0a0a;   /* base background */
  --konjo-deep:     #050505;   /* deeper black for layering */
  --konjo-paper:    #f5f5f5;   /* on-light text */

  --konjo-ice:      #00d4ff;   /* primary cool */
  --konjo-ice-deep: #0088aa;   /* shadowed ice */

  --konjo-ember:    #ff4500;   /* primary warm */
  --konjo-flame:    #ff9500;   /* highlight warm */

  --konjo-jade:     #00ff9d;   /* success/conclusion */
  --konjo-sun:      #ffcc00;   /* warning/testing */
  --konjo-rose:     #ff0066;   /* error/blocker */

  --konjo-mist:     rgba(255,255,255,0.04);  /* subtle borders */
  --konjo-veil:     rgba(255,255,255,0.08);  /* hover state */

  --phase-boot:           var(--konjo-paper);
  --phase-discovery:      var(--konjo-ice);
  --phase-planning:       #00ffd4;
  --phase-implementation: var(--konjo-ember);
  --phase-testing:        var(--konjo-sun);
  --phase-conclusion:     var(--konjo-jade);
}
```

---

## Layout (Hero View)

```
┌──────────────────────────────────────────────────────────────────┐
│  lopi · ⛵ Forge                              ◐ phase wheel  ⚡  │
├──────────────────────────────────────────────────────────────────┤
│                                                                   │
│                                                                   │
│                                                          ┃ token │
│                  ╭─────────────────╮                     ┃ gauge │
│                  │                 │                     ┃       │
│                  │     T H E       │                     ┃ ↑75%  │
│                  │     F O R G E   │                     ┃       │
│                  │                 │                     ┃       │
│                  ╰─────────────────╯                     ┃       │
│                                                                   │
│                                                                   │
│                  thought stream — current agent thinking          │
│                  ──────────────────────────────────────           │
│                                                                   │
├──────────────────────────────────────────────────────────────────┤
│  [logs scrolling…]                              cost: $0.0124    │
└──────────────────────────────────────────────────────────────────┘
```

---

## Implementation Roadmap

### Sprint UI-1 — Foundation + The Forge (this session)
- [x] SvelteKit + TypeScript + TailwindCSS scaffold
- [x] Three.js renderer with custom GLSL Forge shader
- [x] Phase wheel SVG component
- [x] Token gauge component
- [x] Mock data store driving the Forge for standalone demo
- [x] Main hero layout

### Sprint UI-2 — Real Data Integration
- [ ] WebSocket client connected to `/ws/tasks`
- [ ] TaskStatus + AgentEvent types mirroring lopi-core
- [ ] Real Forge driving from live agent state
- [ ] Thought stream wired to agent planning text
- [ ] Log terminal wired to task log

### Sprint UI-3 — Constellation + Multi-Agent
- [ ] Constellation page: all agents orbiting in 3D
- [ ] Click-to-zoom into individual Forge views
- [ ] Cross-agent insights (which agents share patterns?)

### Sprint UI-4 — Production Embed
- [ ] `vite build` produces static assets in `web/dist/`
- [ ] `lopi-ui` Rust crate uses `rust-embed` to bundle them
- [ ] `lopi sail` serves the new UI in place of embedded HTML
- [ ] Mobile responsive (the Forge gracefully degrades)

### Sprint UI-5 — Polish
- [ ] Sound design (optional ambient hum tied to agent state)
- [ ] Keyboard shortcuts (j/k for agent, Escape for overview)
- [ ] Cost analytics view
- [ ] Pattern library browser (memory explorer)

---

## What Makes This Konjo

- **건조 (Dry)** — no React boilerplate, no state libraries, no UI kits. SvelteKit + Three.js + 3 components is the entire dependency surface.
- **ቆንጆ (Beautiful)** — the Forge is itself a piece of art. Custom shader, no off-the-shelf widgets.
- **根性 (Grit)** — we wrote the GLSL ourselves. We tuned the noise frequencies. We chose every color.
- **康宙 (System Health)** — instead of telling you token pressure is at 78%, the sphere shows you. Everyone can read it. The information channel is the visualization.
- **ᨀᨚᨐᨚ (Build)** — this ships as static assets embedded in the Rust binary. One executable, zero deploy steps.

---

## Reference Inspiration

- **Apple Intelligence** — the rainbow ribbon animation that signals AI action
- **Stripe Atlas** — the gradient mesh used as ambient brand presence
- **Linear** — clean, fast, no chrome
- **Anthropic's "thinking" animations** — what attention looks like
- **Three.js noise sphere examples** — surface displacement + GLSL noise
- **NASA mission control** — instruments, not chrome
- **A real fire** — the inspiration for fire shaders, always

*Make it Konjo. Build the ship. Make it seaworthy. Make it beautiful.*
