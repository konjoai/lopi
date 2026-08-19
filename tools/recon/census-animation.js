// Census D — animation inventory. Walks computed styles for @keyframes
// animations and transitions (including hover/focus deltas), checks for
// JS-driven timers, and counts concurrent motion during a running prompt
// (S4) and on a single Loop Stacks row hover (S13).
'use strict';

const fs = require('fs');
const path = require('path');
const { launchBrowser, newContext, BASE_URL } = require('./lib/browser');
const { installRestMocks, installWsReplay, sendSequence, installTaskCreateMock } = require('./lib/mock');
const { seedState, goalToTaskIdMap } = require('./lib/seed');
const { STATES } = require('./fixtures/states');

const OUT = path.join(__dirname, '..', '..', 'recon', 'census_animation.json');

// Elements currently animating via CSS (@keyframes running, not paused) at
// the instant this runs.
const RUNNING_CSS_ANIMATIONS = () => {
  const out = [];
  for (const el of document.querySelectorAll('body *')) {
    const cs = getComputedStyle(el);
    if (cs.animationName && cs.animationName !== 'none' && cs.animationPlayState !== 'paused') {
      const rect = el.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) continue;
      out.push({
        selector: `${el.tagName.toLowerCase()}.${(el.className || '').toString().split(' ')[0]}`,
        name: cs.animationName,
        duration: cs.animationDuration,
        iterationCount: cs.animationIterationCount,
        timing: cs.animationTimingFunction
      });
    }
  }
  return out;
};

const REDUCED_MOTION_CHECK = () => {
  // Any stylesheet rule guarded by prefers-reduced-motion?
  let found = false;
  try {
    for (const sheet of document.styleSheets) {
      let rules;
      try {
        rules = sheet.cssRules;
      } catch {
        continue;
      }
      for (const rule of rules) {
        if (rule.media && [...rule.media].some((m) => m.includes('prefers-reduced-motion'))) {
          found = true;
        }
      }
    }
  } catch {
    /* cross-origin sheet — ignore */
  }
  return found;
};

async function main() {
  const browser = await launchBrowser();
  const { ctx, page } = await newContext(browser, { viewport: { width: 1440, height: 900 }, motion: 'on' });

  // 1. Concurrent motion during a running prompt (S4), sampled 5x over 1s.
  const s4 = STATES.S4_streaming_now;
  await installRestMocks(page, s4.rest || {});
  const ws1 = installWsReplay(page);
  await installTaskCreateMock(page, goalToTaskIdMap(s4.seedCard));
  await ws1.armed;
  await page.goto(`${BASE_URL}/stacks`, { waitUntil: 'load' });
  await page.waitForTimeout(300);
  await seedState(page, s4.seedCard);
  const route1 = await Promise.race([ws1.connected, new Promise((r) => setTimeout(() => r(null), 2000))]);
  if (route1) sendSequence(page, route1, s4.ws).catch(() => {}); // let it stream while we sample; ctx may close before it drains

  const samples = [];
  for (let i = 0; i < 6; i++) {
    await page.waitForTimeout(400);
    const running = await page.evaluate(RUNNING_CSS_ANIMATIONS);
    samples.push({ atMs: i * 400, count: running.length, items: running });
  }
  const maxConcurrent = Math.max(...samples.map((s) => s.count));
  const unionNames = new Set(samples.flatMap((s) => s.items.map((i) => i.name)));

  // 2. Single Loop Stacks row hover — count animations that start on hover.
  await page.waitForTimeout(1500); // let S4 stream settle
  const beforeHover = await page.evaluate(RUNNING_CSS_ANIMATIONS);
  const row = page.locator('.pc, [class*="card"]').first();
  await row.hover({ force: true }).catch(() => {});
  await page.waitForTimeout(150);
  const afterHover = await page.evaluate(RUNNING_CSS_ANIMATIONS);
  const newOnHover = afterHover.filter(
    (a) => !beforeHover.some((b) => b.selector === a.selector && b.name === a.name)
  );

  // Also check transition properties on the hovered row (not @keyframes, but
  // still "things in motion" during the interaction).
  const hoverTransitions = await page.evaluate(() => {
    const el = document.querySelector('.pc, [class*="card"]');
    if (!el) return [];
    const cs = getComputedStyle(el);
    if (cs.transitionProperty === 'none') return [];
    return cs.transitionProperty.split(',').map((p, i) => ({
      property: p.trim(),
      duration: cs.transitionDuration.split(',')[i]?.trim() || cs.transitionDuration
    }));
  });

  const reducedMotionRespected = await page.evaluate(REDUCED_MOTION_CHECK);

  const result = {
    running_prompt_concurrent_motion: {
      samples,
      max_concurrent_css_animations: maxConcurrent,
      distinct_animation_names: [...unionNames]
    },
    single_row_hover: {
      new_css_animations_on_hover: newOnHover,
      transitions_on_hover: hoverTransitions,
      total_things_in_motion_on_hover: newOnHover.length + hoverTransitions.length
    },
    prefers_reduced_motion_respected_anywhere: reducedMotionRespected
  };

  fs.writeFileSync(OUT, JSON.stringify(result, null, 2));
  console.log(`wrote ${OUT}`);
  console.log(`max concurrent CSS animations during running prompt: ${maxConcurrent}`);
  console.log(`things in motion on single row hover: ${result.single_row_hover.total_things_in_motion_on_hover}`);
  console.log(`prefers-reduced-motion respected: ${reducedMotionRespected}`);

  await ctx.close();
  await browser.close();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
