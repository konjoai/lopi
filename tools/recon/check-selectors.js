// Sprint U1 Step 0.3 — pre-flight DOM selector canary check.
'use strict';

const { launchBrowser, newContext, BASE_URL } = require('./lib/browser');
const { installRestMocks, installWsReplay, installTaskCreateMock } = require('./lib/mock');
const { seedState } = require('./lib/seed');
const { STATES } = require('./fixtures/states');

const SELECTORS = ['.gchip', '.runtag', '.sctrl', '.chipinput', '.pc', '.sumln', '.iterpill'];

async function main() {
  const browser = await launchBrowser();
  const { ctx, page } = await newContext(browser, { viewport: { width: 1440, height: 900 }, motion: 'off' });
  const state = STATES.S13_loop_stacks_populated;
  await installRestMocks(page, state.rest || {});
  const ws = installWsReplay(page);
  await installTaskCreateMock(page, {});
  await ws.armed;
  await page.goto(`${BASE_URL}/stacks`, { waitUntil: 'load' });
  await page.waitForTimeout(300);
  await seedState(page, state.seedCard);
  await page.waitForTimeout(300);

  const results = {};
  for (const sel of SELECTORS) {
    results[sel] = await page.locator(sel).count();
  }
  console.log(JSON.stringify(results, null, 2));
  await ctx.close();
  await browser.close();
  const zero = Object.entries(results).filter(([, n]) => n === 0);
  if (zero.length) {
    console.error(`ZERO MATCH: ${zero.map(([s]) => s).join(', ')}`);
    process.exit(1);
  }
}

main().catch((e) => { console.error(e); process.exit(1); });
