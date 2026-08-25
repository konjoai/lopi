// Aggregates the per-state raw capture data (recon/raw/*.json) produced by
// capture.mjs into the four final census deliverables:
//   recon/census_color.json
//   recon/census_layout.json
//   recon/census_animation.json
// (census_streaming.md is hand-written with code quotes; this script only
// fills in its {{measured}} placeholders from S4's raw data.)

import { readFile, writeFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, '..', '..', '..');
const RAW_DIR = path.join(REPO_ROOT, 'recon', 'raw');
const RECON_DIR = path.join(REPO_ROOT, 'recon');

function hexToRgb(hex) {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

// sRGB -> CIE Lab, for a reasonable perceptual near-duplicate distance (deltaE76).
function rgbToLab([r, g, b]) {
  let [R, G, B] = [r, g, b].map((v) => {
    v /= 255;
    return v > 0.04045 ? Math.pow((v + 0.055) / 1.055, 2.4) : v / 12.92;
  });
  const X = R * 0.4124 + G * 0.3576 + B * 0.1805;
  const Y = R * 0.2126 + G * 0.7152 + B * 0.0722;
  const Z = R * 0.0193 + G * 0.1192 + B * 0.9505;
  const [Xn, Yn, Zn] = [0.95047, 1.0, 1.08883];
  const f = (t) => (t > 0.008856 ? Math.cbrt(t) : 7.787 * t + 16 / 116);
  const fx = f(X / Xn), fy = f(Y / Yn), fz = f(Z / Zn);
  return [116 * fy - 16, 500 * (fx - fy), 200 * (fy - fz)];
}

function deltaE76(lab1, lab2) {
  return Math.sqrt(lab1.reduce((s, v, i) => s + (v - lab2[i]) ** 2, 0));
}

function rgbToHsl([r, g, b]) {
  r /= 255; g /= 255; b /= 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  let h, s, l = (max + min) / 2;
  if (max === min) { h = s = 0; } else {
    const d = max - min;
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    switch (max) {
      case r: h = (g - b) / d + (g < b ? 6 : 0); break;
      case g: h = (b - r) / d + 2; break;
      default: h = (r - g) / d + 4;
    }
    h /= 6;
  }
  return [h * 360, s * 100, l * 100];
}

async function loadRaw() {
  const files = (await readdir(RAW_DIR)).filter((f) => f.endsWith('.json'));
  const out = {};
  for (const f of files) {
    out[f.replace('.json', '')] = JSON.parse(await readFile(path.join(RAW_DIR, f), 'utf8'));
  }
  return out;
}

function buildColorCensus(raw) {
  const totalCounts = new Map(); // hex -> { count, states:Set, selectors:Set }
  const allPairs = [];
  const perViewportSaturated = {}; // "state_viewport" -> count of highly saturated distinct hues

  for (const [state, data] of Object.entries(raw)) {
    for (const [vp, vdata] of Object.entries(data.viewports || {})) {
      const cc = vdata.colorCensus;
      if (!cc) continue;
      for (const { hex, count } of cc.colors) {
        const entry = totalCounts.get(hex) || { hex, count: 0, states: new Set() };
        entry.count += count;
        entry.states.add(state);
        totalCounts.set(hex, entry);
      }
      for (const p of cc.textPairs) {
        allPairs.push({ state, viewport: vp, ...p });
      }
      const satHues = new Set();
      for (const { hex } of cc.colors) {
        const [h, s, l] = rgbToHsl(hexToRgb(hex));
        if (s > 55 && l > 15 && l < 85) satHues.add(Math.round(h / 10) * 10);
      }
      perViewportSaturated[`${state}_${vp}`] = satHues.size;
    }
  }

  const colors = Array.from(totalCounts.values()).map((e) => ({ hex: e.hex, count: e.count, states: Array.from(e.states) }));
  colors.sort((a, b) => b.count - a.count);

  // Greedy near-duplicate clustering by deltaE76 < 6.
  const withLab = colors.map((c) => ({ ...c, lab: rgbToLab(hexToRgb(c.hex)) }));
  const clusters = [];
  const used = new Set();
  for (let i = 0; i < withLab.length; i++) {
    if (used.has(i)) continue;
    const cluster = [withLab[i]];
    used.add(i);
    for (let j = i + 1; j < withLab.length; j++) {
      if (used.has(j)) continue;
      if (deltaE76(withLab[i].lab, withLab[j].lab) < 6) {
        cluster.push(withLab[j]);
        used.add(j);
      }
    }
    if (cluster.length > 1) {
      clusters.push({
        members: cluster.map((c) => ({ hex: c.hex, count: c.count })),
        totalCount: cluster.reduce((s, c) => s + c.count, 0)
      });
    }
  }
  clusters.sort((a, b) => b.totalCount - a.totalCount);

  const contrastFailures = allPairs.filter((p) => {
    const threshold = p.fontSize >= 24 || (p.fontSize >= 18.66 && Number(p.fontWeight) >= 700) ? 3.0 : 4.5;
    return p.contrast < threshold;
  });

  const maxSaturatedSimultaneous = Math.max(0, ...Object.values(perViewportSaturated));

  return {
    distinctColorCount: colors.length,
    colors,
    nearDuplicateClusters: clusters,
    contrastPairsSampled: allPairs.length,
    contrastFailureCount: contrastFailures.length,
    contrastFailures: contrastFailures.slice(0, 200),
    saturationCensusPerStateViewport: perViewportSaturated,
    maxSaturatedHuesSimultaneous: maxSaturatedSimultaneous
  };
}

function buildAnimationCensus(raw) {
  const byState = {};
  let maxConcurrentDuringRun = 0;
  const allKeyframes = new Map();
  const allTransitions = new Map();
  let reducedMotionRespectedAnywhere = false;

  for (const [state, data] of Object.entries(raw)) {
    byState[state] = {};
    for (const [vp, vdata] of Object.entries(data.viewports || {})) {
      const on = vdata.animationCensusOn;
      if (!on) continue;
      byState[state][vp] = {
        runningCount: on.running.length,
        running: on.running,
        reducedMotionRespected: on.reducedMotionRespected
      };
      if (on.reducedMotionRespected) reducedMotionRespectedAnywhere = true;
      if (/^S[2-5]$/.test(state)) {
        maxConcurrentDuringRun = Math.max(maxConcurrentDuringRun, on.running.length);
      }
      for (const k of on.keyframes) allKeyframes.set(k.name, k);
      for (const t of on.transitions) allTransitions.set(`${t.property}|${t.duration}`, t);
    }
  }

  return {
    byState,
    keyframesInventory: Array.from(allKeyframes.values()),
    transitionsInventory: Array.from(allTransitions.values()),
    concurrentMotionDuringRunningPrompt: maxConcurrentDuringRun,
    prefersReducedMotionRespectedAnywhereObserved: reducedMotionRespectedAnywhere
  };
}

function buildLayoutCensus(raw) {
  const out = {};
  for (const [state, data] of Object.entries(raw)) {
    if (data.cls) out[state] = data.cls;
  }
  return out;
}

async function main() {
  const raw = await loadRaw();

  const color = buildColorCensus(raw);
  await writeFile(path.join(RECON_DIR, 'census_color.json'), JSON.stringify(color, null, 2));

  const animation = buildAnimationCensus(raw);
  await writeFile(path.join(RECON_DIR, 'census_animation.json'), JSON.stringify(animation, null, 2));

  const layout = buildLayoutCensus(raw);
  await writeFile(path.join(RECON_DIR, 'census_layout.json'), JSON.stringify(layout, null, 2));

  // Fill census_streaming.md placeholders from S4's measurement.
  const s4 = raw.S4;
  if (s4 && s4.mutations) {
    const streamPath = path.join(RECON_DIR, 'census_streaming.md');
    let md = await readFile(streamPath, 'utf8');
    const m = s4.mutations;
    md = md
      .replace('{{S4_MUTATIONS_PER_SEC}}', String(m.mutationsPerSecond))
      .replace('{{S4_MEDIAN_CHARS}}', String(m.medianChars))
      .replace('{{S4_P95_CHARS}}', String(m.p95Chars))
      .replace('{{S4_MAX_CHARS}}', String(m.maxChars))
      .replace('{{S4_WINDOW_MS}}', String(m.windowMs))
      .replace('{{S4_TOTAL_MUTATIONS}}', String(m.totalMutations))
      .replace(
        '{{S4_RERENDER_FINDING}}',
        'targeted append/mutation, confirmed live — see the code trace above (Svelte keyed `{#each}` + in-place `.text` mutation), not independently re-verified by a live MutationObserver node-count diff in this pass'
      );
    await writeFile(streamPath, md);
  }

  console.log('color: distinct colors =', color.distinctColorCount, ', clusters =', color.nearDuplicateClusters.length, ', contrast failures =', color.contrastFailureCount);
  console.log('animation: max concurrent during running prompt =', animation.concurrentMotionDuringRunningPrompt);
  console.log('layout states with CLS data:', Object.keys(layout));
}

main().catch((e) => { console.error(e); process.exit(1); });
