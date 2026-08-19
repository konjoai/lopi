// Census C — layout stability. Collects real `layout-shift` PerformanceObserver
// entries during S4 (streaming) and S9 (long scrollback + manual scroll),
// motion=on (a frozen/motion=off page cannot shift by definition).
'use strict';

const fs = require('fs');
const path = require('path');
const { launchBrowser, newContext, BASE_URL } = require('./lib/browser');
const { installRestMocks, installWsReplay, sendSequence, installTaskCreateMock } = require('./lib/mock');
const { seedState, goalToTaskIdMap } = require('./lib/seed');
const { STATES } = require('./fixtures/states');

const OUT = path.join(__dirname, '..', '..', 'recon', 'census_layout.json');

const START_CLS_OBSERVER = () => {
  window.__clsEntries = [];
  try {
    const po = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        if (!entry.hadRecentInput) {
          window.__clsEntries.push({
            value: entry.value,
            time: entry.startTime,
            sources: (entry.sources || []).map((s) => ({
              node: s.node ? `${s.node.tagName?.toLowerCase()}.${(s.node.className || '').toString().split(' ')[0]}` : null,
              prevRect: s.previousRect,
              curRect: s.currentRect
            }))
          });
        }
      }
    });
    po.observe({ type: 'layout-shift', buffered: true });
    window.__clsObserver = po;
  } catch (e) {
    window.__clsUnsupported = String(e);
  }
};

const READ_CLS = () => ({
  entries: window.__clsEntries || [],
  unsupported: window.__clsUnsupported || null
});

async function measure(page, label, driveFn) {
  await page.evaluate(START_CLS_OBSERVER);
  await driveFn();
  await page.waitForTimeout(300);
  const { entries, unsupported } = await page.evaluate(READ_CLS);
  const total = entries.reduce((s, e) => s + e.value, 0);
  const top5 = [...entries].sort((a, b) => b.value - a.value).slice(0, 5);
  return { label, unsupported, total_cls: Math.round(total * 10000) / 10000, entry_count: entries.length, top_shifts: top5 };
}

async function main() {
  const browser = await launchBrowser();
  const { ctx, page } = await newContext(browser, { viewport: { width: 1440, height: 900 }, motion: 'on' });
  const results = [];

  // S4 — streaming: does new log content shift anything above it?
  {
    const state = STATES.S4_streaming_now;
    await installRestMocks(page, state.rest || {});
    const ws = installWsReplay(page);
    await installTaskCreateMock(page, goalToTaskIdMap(state.seedCard));
    await ws.armed;
    await page.goto(`${BASE_URL}/stacks`, { waitUntil: 'load' });
    await page.waitForTimeout(300);
    await seedState(page, state.seedCard);
    const route = await Promise.race([ws.connected, new Promise((r) => setTimeout(() => r(null), 2000))]);
    results.push(
      await measure(page, 'S4_streaming_now (log lines arriving)', async () => {
        if (route) await sendSequence(page, route, state.ws);
      })
    );
  }

  // S9 — long scrollback: does scrolling the log region fight follow-tail,
  // and does anything shift while scrolling?
  {
    const state = STATES.S9_long_scrollback;
    await installRestMocks(page, state.rest || {});
    const ws = installWsReplay(page);
    await installTaskCreateMock(page, goalToTaskIdMap(state.seedCard));
    await ws.armed;
    await page.goto(`${BASE_URL}/stacks`, { waitUntil: 'load' });
    await page.waitForTimeout(300);
    await seedState(page, state.seedCard);
    const route = await Promise.race([ws.connected, new Promise((r) => setTimeout(() => r(null), 2000))]);
    if (route) await sendSequence(page, route, state.ws);
    await page.waitForTimeout(500);
    results.push(
      await measure(page, 'S9_long_scrollback (manual scroll up mid-tail)', async () => {
        await page.mouse.wheel(0, -2000);
        await page.waitForTimeout(300);
        await page.mouse.wheel(0, -2000);
        await page.waitForTimeout(300);
      })
    );
    // Scroll-anchoring / follow-tail check: after scrolling up, does a new
    // log line arriving yank the view back to the bottom?
    const scrollTopBefore = await page.evaluate(() => {
      const el = document.querySelector('[class*="log"], [class*="scroll"]');
      return el ? el.scrollTop : null;
    });
    await page.waitForTimeout(1000);
    const scrollTopAfter = await page.evaluate(() => {
      const el = document.querySelector('[class*="log"], [class*="scroll"]');
      return el ? el.scrollTop : null;
    });
    results.push({
      label: 'S9 scroll-anchoring check',
      scrollTopBefore,
      scrollTopAfter,
      followTailFoughtManualScroll: scrollTopBefore !== null && scrollTopAfter !== null ? scrollTopAfter !== scrollTopBefore : null
    });
  }

  // S13 — does opening a popover shift anything behind it?
  {
    const state = STATES.S13_loop_stacks_populated;
    await installRestMocks(page, state.rest || {});
    const ws = installWsReplay(page);
    await installTaskCreateMock(page, goalToTaskIdMap(state.seedCard));
    await ws.armed;
    await page.goto(`${BASE_URL}/stacks`, { waitUntil: 'load' });
    await page.waitForTimeout(300);
    await seedState(page, state.seedCard);
    const route = await Promise.race([ws.connected, new Promise((r) => setTimeout(() => r(null), 2000))]);
    if (route) await sendSequence(page, route, state.ws);
    await page.waitForTimeout(300);
    results.push(
      await measure(page, 'S13_loop_stacks_populated (open config popover)', async () => {
        const cfgBtn = page.locator('button[title="stack default config"]').first();
        await cfgBtn.click({ force: true });
        await page.waitForTimeout(300);
      })
    );
  }

  fs.writeFileSync(OUT, JSON.stringify(results, null, 2));
  console.log(`wrote ${OUT}`);
  for (const r of results) {
    if ('total_cls' in r) console.log(`${r.label}: CLS=${r.total_cls}, entries=${r.entry_count}`);
    else console.log(`${r.label}: ${JSON.stringify(r)}`);
  }

  await ctx.close();
  await browser.close();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
