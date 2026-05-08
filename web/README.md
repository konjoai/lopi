# lopi web — The Forge

> Live cognitive visualization for lopi agents. A morphing sphere of fire and ice that breathes with the state of every agent in flight.

## Stack
- **SvelteKit 2** + TypeScript
- **Three.js** with custom GLSL fragment + vertex shaders
- **TailwindCSS** with Konjo palette
- **WebSocket** to lopi-ui's `/ws/tasks` (proxied in dev to `localhost:3000`)

## Quickstart

```bash
cd web
npm install
npm run dev          # opens http://localhost:5173 with hot-reload
```

If `lopi sail` is running on `:3000`, the dashboard connects automatically.
If not, the UI runs on simulated mock data so you can preview the visuals
without a backend.

## Build for production

```bash
npm run build        # produces static assets in web/dist/
```

The `dist/` directory is intended to be embedded into the lopi Rust binary
via `rust-embed` so `lopi sail` ships a single executable.

## Project map

```
web/
├── src/
│   ├── lib/
│   │   ├── forge/
│   │   │   └── Forge.svelte          ← the centerpiece (custom GLSL shader)
│   │   ├── components/
│   │   │   ├── PhaseWheel.svelte     ← circular phase indicator
│   │   │   ├── TokenGauge.svelte     ← context pressure bar
│   │   │   ├── ThoughtStream.svelte  ← typewriter agent planning text
│   │   │   ├── AgentCard.svelte      ← sidebar list item
│   │   │   ├── LogStream.svelte      ← terminal log viewer
│   │   │   └── CostCounter.svelte    ← animated USD counter
│   │   └── stores/
│   │       └── agents.ts             ← state + WebSocket + mock generator
│   ├── routes/
│   │   ├── +layout.svelte            ← top bar + connection indicator
│   │   ├── +layout.ts                ← static-adapter config
│   │   └── +page.svelte              ← hero layout
│   ├── app.css                       ← Konjo theme + globals
│   └── app.html                      ← HTML shell
├── package.json
├── svelte.config.js                  ← static adapter → web/dist/
├── tailwind.config.js                ← Konjo palette
└── vite.config.js                    ← proxy /ws + /api to localhost:3000
```

## Design

See [`../LOPI_UI_VISION.md`](../LOPI_UI_VISION.md) for the full vision document.

The Forge is the headline feature: a sphere driven by three layers of GLSL —
volumetric simplex noise, fire/ice domain coloring, and a Fresnel aura tinted
by the active agent's phase color. Every visual property is wired to a real
agent metric: token pressure → turbulence, phase → aura, tokens/sec → pulse.

## License
MIT © KonjoAI
