// Step 3 — page-state screenshot protocol. Captures every fixture state
// (fixtures/states.js) at the five page-level viewports, motion=off
// (the layout/colour/contrast reference stills). Motion=on frame strips are
// a separate, narrower deliverable — see capture-motion.js — scoped to the
// states that actually have live motion, per the brief's own "otherwise the
// shot count explodes into something nobody will look at" rationale.
'use strict';

const path = require('path');
const fs = require('fs');
const { launchBrowser, newContext, BASE_URL } = require('./lib/browser');
const { installRestMocks, installWsReplay, sendSequence, installTaskCreateMock } = require('./lib/mock');
const { seedState, goalToTaskIdMap } = require('./lib/seed');
const { STATES } = require('./fixtures/states');

const VIEWPORTS = [
  { name: '1920x1080', width: 1920, height: 1080 },
  { name: '1440x900', width: 1440, height: 900 },
  { name: '1280x800', width: 1280, height: 800 },
  { name: '768x1024', width: 768, height: 1024 },
  { name: '390x844', width: 390, height: 844 }
];

const OUT_DIR = path.join(__dirname, '..', '..', 'recon', 'shots', 'pages');

async function captureOne(browser, stateKey, state, viewport, results) {
  if (state.unreachable) {
    results.push({ state: stateKey, viewport: viewport.name, status: 'unreachable', note: state.unreachable });
    return;
  }
  const { ctx, page } = await newContext(browser, { viewport, motion: 'off' });
  try {
    await installRestMocks(page, state.rest || {});
    const ws = installWsReplay(page);
    await installTaskCreateMock(page, goalToTaskIdMap(state.seedCard));
    await ws.armed;

    await page.goto(`${BASE_URL}/stacks`, { waitUntil: 'load' });
    await page.waitForTimeout(400);

    await seedState(page, state.seedCard);

    if (state.ws && state.ws.length) {
      const wsRoute = await Promise.race([
        ws.connected,
        new Promise((resolve) => setTimeout(() => resolve(null), 2000))
      ]);
      if (wsRoute) {
        await sendSequence(page, wsRoute, state.ws);
      } else {
        results.push({ state: stateKey, viewport: viewport.name, status: 'ws-not-connected', note: 'WS route never armed before timeout' });
      }
    }

    await page.waitForTimeout(250);
    const file = path.join(OUT_DIR, `${stateKey}_${viewport.name}.png`);
    await page.screenshot({ path: file, fullPage: false });
    results.push({ state: stateKey, viewport: viewport.name, status: 'ok', file: path.relative(path.join(__dirname, '..', '..'), file) });
  } catch (err) {
    results.push({ state: stateKey, viewport: viewport.name, status: 'error', note: String(err && err.message || err) });
  } finally {
    await ctx.close();
  }
}

async function main() {
  fs.mkdirSync(OUT_DIR, { recursive: true });
  const browser = await launchBrowser();
  const results = [];
  const only = process.argv[2] ? process.argv[2].split(',') : null;

  for (const [stateKey, state] of Object.entries(STATES)) {
    if (only && !only.includes(stateKey)) continue;
    for (const viewport of VIEWPORTS) {
      process.stdout.write(`capturing ${stateKey} @ ${viewport.name} ... `);
      await captureOne(browser, stateKey, state, viewport, results);
      console.log(results[results.length - 1].status);
    }
  }

  await browser.close();
  fs.writeFileSync(
    path.join(__dirname, '..', '..', 'recon', 'capture_log.json'),
    JSON.stringify(results, null, 2)
  );
  const failed = results.filter((r) => r.status !== 'ok' && r.status !== 'unreachable');
  console.log(`\n${results.length} captures, ${failed.length} non-ok.`);
  if (failed.length) console.log(JSON.stringify(failed, null, 2));
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
