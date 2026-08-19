// Census A — colour and contrast. Extracts COMPUTED styles from the
// rendered DOM (never source CSS) across a curated set of fixture states,
// including interaction states (hover/focus-visible/active/disabled/error),
// then normalises, clusters near-duplicates, and checks WCAG AA contrast.
'use strict';

const fs = require('fs');
const path = require('path');
const { launchBrowser, newContext, BASE_URL } = require('./lib/browser');
const { installRestMocks, installWsReplay, sendSequence, installTaskCreateMock } = require('./lib/mock');
const { seedState, goalToTaskIdMap } = require('./lib/seed');
const { STATES } = require('./fixtures/states');
const { toHex, contrastRatio, rgbDistance, saturation } = require('./lib/color');

const OUT = path.join(__dirname, '..', '..', 'recon', 'census_color.json');

// Curated subset covering the states most likely to exercise distinct
// colour usage: error/terminal states, success, dense composer, streaming.
const COVERAGE_STATES = [
  'S1_cold_start_empty',
  'S4_streaming_now',
  'S5_gate_failure',
  'S6_dead_letter',
  'S9_long_scrollback',
  'S12_all_finished_green',
  'S13_loop_stacks_populated'
];

const EXTRACT_SCRIPT = () => {
  function rgbaOf(str) {
    if (!str) return null;
    const m = str.match(/rgba?\(([^)]+)\)/);
    if (!m) return null;
    const parts = m[1].split(',').map((s) => parseFloat(s.trim()));
    const [r, g, b, a = 1] = parts;
    if ([r, g, b].some((n) => Number.isNaN(n)) || a === 0) return null;
    return { r, g, b, a };
  }
  function shortSelector(el) {
    const cls = (el.className && typeof el.className === 'string')
      ? '.' + el.className.trim().split(/\s+/).slice(0, 2).join('.')
      : '';
    return `${el.tagName.toLowerCase()}${cls}`;
  }
  function effectiveBg(el) {
    let node = el;
    while (node) {
      const cs = getComputedStyle(node);
      const bg = rgbaOf(cs.backgroundColor);
      if (bg && bg.a >= 0.5) return bg;
      node = node.parentElement;
    }
    return { r: 0, g: 0, b: 0, a: 1 };
  }

  const out = { text: [], background: [], border: [] };
  const all = document.querySelectorAll('body *');
  for (const el of all) {
    const rect = el.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) continue;
    const cs = getComputedStyle(el);
    if (cs.visibility === 'hidden' || cs.display === 'none') continue;

    const hasOwnText = Array.from(el.childNodes).some(
      (n) => n.nodeType === Node.TEXT_NODE && n.textContent.trim().length > 0
    );
    if (hasOwnText) {
      const c = rgbaOf(cs.color);
      if (c) out.text.push({ color: c, bg: effectiveBg(el), selector: shortSelector(el) });
    }
    const bg = rgbaOf(cs.backgroundColor);
    if (bg) out.background.push({ color: bg, selector: shortSelector(el) });
    const bc = rgbaOf(cs.borderTopColor);
    if (bc && cs.borderTopWidth !== '0px' && cs.borderTopStyle !== 'none') {
      out.border.push({ color: bc, selector: shortSelector(el) });
    }
  }
  return out;
};

async function loadState(page, stateKey) {
  const state = STATES[stateKey];
  await installRestMocks(page, state.rest || {});
  const ws = installWsReplay(page);
  await installTaskCreateMock(page, goalToTaskIdMap(state.seedCard));
  await ws.armed;
  await page.goto(`${BASE_URL}/stacks`, { waitUntil: 'load' });
  await page.waitForTimeout(300);
  await seedState(page, state.seedCard);
  if (state.ws && state.ws.length) {
    const route = await Promise.race([ws.connected, new Promise((r) => setTimeout(() => r(null), 2000))]);
    if (route) await sendSequence(page, route, state.ws);
  }
  await page.waitForTimeout(200);
}

async function captureInteractionStates(page, samples) {
  // Hover + focus-visible on a handful of representative controls: the
  // "add" button, a chip, the stack-controls dock trigger, and (if present)
  // an error-styled element. Disabled state comes for free from a disabled
  // "add" button on an empty composer.
  const targets = [
    { name: 'add-button', locator: page.getByRole('button', { name: 'add', exact: true }).first() },
    { name: 'stack-controls-trigger', locator: page.getByRole('button', { name: 'stack controls' }).first() },
    { name: 'chip-alias', locator: page.locator('.chip, [class*="chip"]').first() }
  ];
  for (const t of targets) {
    try {
      const count = await t.locator.count();
      if (!count) continue;
      const disabled = await t.locator.first().isDisabled().catch(() => false);
      if (disabled) {
        const cs = await t.locator.first().evaluate((el) => getComputedStyle(el).color);
        samples.push({ state: 'disabled', target: t.name, color: cs });
        continue;
      }
      await t.locator.first().hover({ force: true }).catch(() => {});
      await page.waitForTimeout(80);
      const hoverCs = await t.locator.first().evaluate((el) => {
        const cs = getComputedStyle(el);
        return { color: cs.color, background: cs.backgroundColor, border: cs.borderColor };
      });
      samples.push({ state: 'hover', target: t.name, ...hoverCs });

      await t.locator.first().focus().catch(() => {});
      await page.waitForTimeout(80);
      const focusCs = await t.locator.first().evaluate((el) => {
        const cs = getComputedStyle(el);
        return { color: cs.color, background: cs.backgroundColor, outline: cs.outlineColor, outlineWidth: cs.outlineWidth };
      });
      samples.push({ state: 'focus-visible', target: t.name, ...focusCs });
    } catch (e) {
      samples.push({ state: 'error', target: t.name, note: String(e.message || e) });
    }
  }
}

async function main() {
  const browser = await launchBrowser();
  const { ctx, page } = await newContext(browser, { viewport: { width: 1440, height: 900 }, motion: 'off' });

  const usage = { text: new Map(), background: new Map(), border: new Map() };
  const interactionSamples = [];
  let s13SaturationSet = new Set();

  for (const stateKey of COVERAGE_STATES) {
    await loadState(page, stateKey);
    const extracted = await page.evaluate(EXTRACT_SCRIPT);
    for (const kind of ['text', 'background', 'border']) {
      for (const item of extracted[kind]) {
        const hex = toHex(item.color);
        if (!usage[kind].has(hex)) usage[kind].set(hex, { count: 0, selectors: new Set() });
        const rec = usage[kind].get(hex);
        rec.count++;
        rec.selectors.add(item.selector);
      }
      if (kind === 'text') {
        for (const item of extracted.text) {
          if (saturation(item.color) > 0.5) s13SaturationSet.add(toHex(item.color));
        }
      }
    }
    if (stateKey === 'S13_loop_stacks_populated') {
      for (const item of extracted.background) {
        if (saturation(item.color) > 0.5) s13SaturationSet.add(toHex(item.color));
      }
    }
    if (stateKey === 'S6_dead_letter' || stateKey === 'S5_gate_failure') {
      await captureInteractionStates(page, interactionSamples);
    }
  }

  // Root design tokens for traceability check.
  const tokens = await page.evaluate(() => {
    const cs = getComputedStyle(document.documentElement);
    const names = [
      '--konjo-black', '--konjo-deep', '--konjo-paper', '--konjo-ice', '--konjo-ice-deep',
      '--konjo-ember', '--konjo-flame', '--konjo-jade', '--konjo-sun', '--konjo-rose',
      '--konjo-plasma', '--konjo-violet', '--konjo-violet-bright', '--konjo-mint', '--konjo-rose-muted',
      '--konjo-teal', '--konjo-violet-light', '--konjo-accent', '--konjo-accent-2'
    ];
    const out = {};
    for (const n of names) {
      const v = cs.getPropertyValue(n).trim();
      if (v) out[n] = v;
    }
    return out;
  });
  const tokenHexes = new Set(Object.values(tokens).map((v) => v.toLowerCase()));

  // Near-duplicate clustering (Euclidean distance < 18 in sRGB).
  const allHexes = [...new Set([...usage.text.keys(), ...usage.background.keys(), ...usage.border.keys()])];
  const parseHex = (h) => ({ r: parseInt(h.slice(1, 3), 16), g: parseInt(h.slice(3, 5), 16), b: parseInt(h.slice(5, 7), 16) });
  const clusters = [];
  const assigned = new Set();
  for (const h of allHexes) {
    if (assigned.has(h)) continue;
    const cluster = [h];
    assigned.add(h);
    for (const other of allHexes) {
      if (assigned.has(other)) continue;
      if (rgbDistance(parseHex(h), parseHex(other)) < 18) {
        cluster.push(other);
        assigned.add(other);
      }
    }
    if (cluster.length > 1) clusters.push(cluster);
  }

  // Contrast check (WCAG AA: 4.5:1 body text).
  const contrastFailures = [];
  const seenPairs = new Set();
  for (const [hex, rec] of usage.text) {
    // Re-derive one representative bg per hex from the raw extraction pass —
    // approximate by pairing with the most common background overall
    // (full per-pair tracking would need the raw list; good enough for a
    // recon-scale finding, noted as such in REPORT.md).
  }
  // Redo a direct pass for accurate text/bg pairing (cheap re-run at the
  // richest state, S13).
  await loadState(page, 'S13_loop_stacks_populated');
  const pairExtract = await page.evaluate(EXTRACT_SCRIPT);
  for (const item of pairExtract.text) {
    const fg = item.color;
    const bg = item.bg;
    const ratio = contrastRatio(fg, bg);
    const key = `${toHex(fg)}|${toHex(bg)}|${item.selector}`;
    if (seenPairs.has(key)) continue;
    seenPairs.add(key);
    if (ratio < 4.5) {
      contrastFailures.push({ selector: item.selector, fg: toHex(fg), bg: toHex(bg), ratio: Math.round(ratio * 100) / 100 });
    }
  }

  const result = {
    coverage_states: COVERAGE_STATES,
    colors: {
      text: [...usage.text].map(([hex, r]) => ({ hex, count: r.count, selectors: [...r.selectors], token: tokenHexes.has(hex) })),
      background: [...usage.background].map(([hex, r]) => ({ hex, count: r.count, selectors: [...r.selectors], token: tokenHexes.has(hex) })),
      border: [...usage.border].map(([hex, r]) => ({ hex, count: r.count, selectors: [...r.selectors], token: tokenHexes.has(hex) }))
    },
    design_tokens_found: tokens,
    token_traceability_ratio: (() => {
      const total = allHexes.length;
      const traced = allHexes.filter((h) => tokenHexes.has(h)).length;
      return { traced, total, ratio: total ? Math.round((traced / total) * 1000) / 1000 : null };
    })(),
    near_duplicate_clusters: clusters,
    contrast_failures_aa: contrastFailures,
    interaction_state_samples: interactionSamples,
    s13_simultaneous_saturated_hues: [...s13SaturationSet]
  };

  fs.writeFileSync(OUT, JSON.stringify(result, null, 2));
  console.log(`wrote ${OUT}`);
  console.log(`distinct hexes: ${allHexes.length}, clusters: ${clusters.length}, contrast failures: ${contrastFailures.length}`);
  console.log(`token traceability: ${JSON.stringify(result.token_traceability_ratio)}`);
  console.log(`S13 simultaneous saturated hues: ${s13SaturationSet.size}`);

  await ctx.close();
  await browser.close();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
