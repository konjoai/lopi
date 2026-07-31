// Sprint U1 — parameterized page-state + component-grid capture, reusing the
// Sprint U-0 (PR #188) harness's lib/fixtures verbatim. Writes into
// recon/u1/<OUT_NAME>/ instead of clobbering the U-0 census output under
// recon/shots/. Usage: node u1-capture.js <out-name> [stateKey,stateKey,...]
'use strict';

const path = require('path');
const fs = require('fs');
const { launchBrowser, newContext, BASE_URL } = require('./lib/browser');
const { installRestMocks, installWsReplay, sendSequence, installTaskCreateMock } = require('./lib/mock');
const { seedState, goalToTaskIdMap } = require('./lib/seed');
const { STATES } = require('./fixtures/states');

const VIEWPORTS = [
  { name: '1440x900', width: 1440, height: 900 },
  { name: '390x844', width: 390, height: 844 }
];

const OUT_NAME = process.argv[2];
if (!OUT_NAME) {
  console.error('usage: node u1-capture.js <out-name> [stateKey,stateKey,...]');
  process.exit(1);
}
const ONLY = process.argv[3] ? process.argv[3].split(',') : null;

const ROOT = path.join(__dirname, '..', '..', 'recon', 'u1', OUT_NAME);
const PAGES_DIR = path.join(ROOT, 'pages');
const COMPONENTS_DIR = path.join(ROOT, 'components');

async function capturePageState(browser, stateKey, state, viewport, results) {
  if (state.unreachable) {
    results.push({ state: stateKey, viewport: viewport.name, status: 'unreachable', note: state.unreachable });
    return;
  }
  // suppressFocusRing: page-state shots are meant to show fixture content at
  // rest, not whatever element the scripted seeding clicks last landed on
  // (see LEDGER.md Sprint U1 "capture timing non-determinism" — a stray
  // :focus-visible ring on that element is real but non-deterministic).
  // Component grids (captureComponentsAtViewport, below) deliberately
  // capture focus-visible cells and do NOT set this.
  const { ctx, page } = await newContext(browser, { viewport, motion: 'off', suppressFocusRing: true });
  try {
    await installRestMocks(page, state.rest || {});
    const ws = installWsReplay(page);
    await installTaskCreateMock(page, goalToTaskIdMap(state.seedCard));
    await ws.armed;

    await page.goto(`${BASE_URL}/stacks`, { waitUntil: 'load' });
    await page.waitForLoadState('networkidle').catch(() => {});
    await page.waitForTimeout(500);

    await seedState(page, state.seedCard);
    // Extra settle after seeding, before any WS traffic: multi-pane states
    // (S3/S11/S12) add panes mid-sequence (lib/seed.js addPane), and pane
    // mount uses a Svelte transition: directive — inline-style-driven, not
    // touched by MOTION_OFF_CSS's `transition: none` override — so layout
    // can still be settling when the next scripted click fires, landing on
    // a dropdown/menu toggle instead of its intended target. See LEDGER.md
    // Sprint U1 "capture timing non-determinism".
    await page.waitForTimeout(400);
    await page.keyboard.press('Escape').catch(() => {});

    if (state.ws && state.ws.length) {
      const wsRoute = await Promise.race([
        ws.connected,
        new Promise((resolve) => setTimeout(() => resolve(null), 2000))
      ]);
      if (wsRoute) {
        await sendSequence(page, wsRoute, state.ws);
      } else {
        results.push({ state: stateKey, viewport: viewport.name, status: 'ws-not-connected' });
      }
    }

    // Fixed post-sequence settle wait — deliberately generous (was 250ms,
    // caused real Svelte-reactivity capture races: see LEDGER.md Sprint U1
    // "capture timing non-determinism"). Every fixture WireMessage carries
    // atMs=0 or small fixed deltas, so this only affects wall-clock capture
    // reliability, not fixture content. States with several concurrent
    // panes (S3, S11, S12) need more margin than a single-pane state.
    await page.waitForTimeout(2000);
    // Sprint U1 finding (LEDGER.md "capture timing non-determinism", part 2):
    // suppressFocusRing alone didn't fully explain S11/S12's remaining
    // diffs — the actual cause is Playwright's synthetic mouse cursor
    // staying parked at the coordinates of seedState's last force-click
    // (lib/seed.js's runPane toggles the stack-controls dock via a real
    // mouse click), so a :hover rule can match unpredictably depending on
    // layout. Move the mouse to a neutral corner before every shot rather
    // than suppressing :hover CSS — a real :hover colour bug should still
    // fail this gate, an accidental leftover cursor position should not.
    await page.mouse.move(0, 0);
    await page.waitForTimeout(100);
    const file = path.join(PAGES_DIR, `${stateKey}_${viewport.name}.png`);
    await page.screenshot({ path: file, fullPage: false, timeout: 120000 });
    results.push({ state: stateKey, viewport: viewport.name, status: 'ok', file: path.relative(ROOT, file) });
  } catch (err) {
    results.push({ state: stateKey, viewport: viewport.name, status: 'error', note: String((err && err.message) || err) });
  } finally {
    await ctx.close();
  }
}

async function shot(locator, filePath) {
  await locator.screenshot({ path: filePath }).catch(async () => {
    await locator.page().screenshot({ path: filePath });
  });
}

async function buildGrid(page, componentName, viewportName, cells) {
  const html = `<!doctype html><html><head><style>
    body { margin:0; background:#111; font-family: monospace; color:#eee; }
    .grid { display:flex; flex-wrap:wrap; gap:12px; padding:12px; }
    .cell { border:1px solid #444; padding:6px; background:#1a1a1a; }
    .cell img { display:block; max-width:320px; }
    .cap { font-size:11px; padding-top:4px; color:#9cf; }
  </style></head><body><div class="grid">
    ${cells.map((c) => `<div class="cell"><img src="file://${c.file}"><div class="cap">${c.label}</div></div>`).join('\n')}
  </div></body></html>`;
  const tmpHtml = path.join(COMPONENTS_DIR, `_grid_${componentName}_${viewportName}.html`);
  fs.writeFileSync(tmpHtml, html);
  const gridPage = await page.context().newPage();
  await gridPage.setViewportSize({ width: 1400, height: 1000 });
  await gridPage.goto(`file://${tmpHtml}`);
  await gridPage.waitForTimeout(150);
  const outFile = path.join(COMPONENTS_DIR, `${componentName}_${viewportName}.png`);
  await gridPage.locator('.grid').screenshot({ path: outFile });
  await gridPage.close();
  fs.unlinkSync(tmpHtml);
  return outFile;
}

async function captureComponentsAtViewport(browser, viewport, manifest) {
  const { ctx, page } = await newContext(browser, { viewport, motion: 'off' });
  const state = STATES.S13_loop_stacks_populated;
  await installRestMocks(page, state.rest || {});
  const ws = installWsReplay(page);
  await installTaskCreateMock(page, {});
  await ws.armed;
  await page.goto(`${BASE_URL}/stacks`, { waitUntil: 'load' });
  await page.waitForTimeout(300);
  await seedState(page, state.seedCard);
  await page.waitForTimeout(200);

  const tmpDir = path.join(COMPONENTS_DIR, '_tmp');
  fs.mkdirSync(tmpDir, { recursive: true });

  // config button
  {
    const cells = [];
    const addBtn = page.getByRole('button', { name: 'add', exact: true }).first();
    const f1 = path.join(tmpDir, 'add_rest.png');
    await shot(addBtn, f1);
    cells.push({ file: f1, label: 'rest' });
    await addBtn.hover({ force: true });
    await page.waitForTimeout(80);
    const f2 = path.join(tmpDir, 'add_hover.png');
    await shot(addBtn, f2);
    cells.push({ file: f2, label: 'hover' });
    await addBtn.focus().catch(() => {});
    await page.waitForTimeout(80);
    const f3 = path.join(tmpDir, 'add_focus.png');
    await shot(addBtn, f3);
    cells.push({ file: f3, label: 'focus-visible' });
    const secondPaneAdd = page.locator('.pane').nth(1).getByRole('button', { name: 'add', exact: true }).first();
    const f4 = path.join(tmpDir, 'add_disabled.png');
    await shot(secondPaneAdd, f4);
    cells.push({ file: f4, label: 'disabled' });
    const gridFile = await buildGrid(page, 'config_button', viewport.name, cells);
    manifest.push({ component: 'config-button', viewport: viewport.name, grid: path.relative(ROOT, gridFile) });
  }

  // schedule popover
  {
    const cells = [];
    const pane = page.locator('.pane').first();
    const schedBtn = pane.locator('button[title="schedule the stack"]').first();
    const f1 = path.join(tmpDir, 'sched_trigger_rest.png');
    await shot(schedBtn, f1);
    cells.push({ file: f1, label: 'trigger rest' });
    await schedBtn.hover({ force: true });
    await page.waitForTimeout(80);
    const f2 = path.join(tmpDir, 'sched_trigger_hover.png');
    await shot(schedBtn, f2);
    cells.push({ file: f2, label: 'trigger hover' });
    await schedBtn.click({ force: true });
    await page.waitForTimeout(200);
    const popover = page.locator('.pop.sched').first();
    const f3 = path.join(tmpDir, 'sched_open.png');
    await shot(popover, f3);
    cells.push({ file: f3, label: 'open' });
    await page.keyboard.press('Escape');
    await page.waitForTimeout(150);
    const gridFile = await buildGrid(page, 'schedule_popover', viewport.name, cells);
    manifest.push({ component: 'schedule popover', viewport: viewport.name, grid: path.relative(ROOT, gridFile) });
  }

  // config popover
  {
    const cells = [];
    const pane = page.locator('.pane').first();
    const cfgBtn = pane.locator('button[title="stack default config"]').first();
    const f1 = path.join(tmpDir, 'cfg_trigger_rest.png');
    await shot(cfgBtn, f1);
    cells.push({ file: f1, label: 'trigger rest' });
    await cfgBtn.click({ force: true });
    await page.waitForTimeout(200);
    const popover = page.locator('.pop').filter({ has: page.locator('.cfgrow') }).first();
    const f2 = path.join(tmpDir, 'cfg_open.png');
    await shot(popover, f2);
    cells.push({ file: f2, label: 'open' });
    const gridFile = await buildGrid(page, 'config_popover', viewport.name, cells);
    manifest.push({ component: 'config popover', viewport: viewport.name, grid: path.relative(ROOT, gridFile) });
    await page.keyboard.press('Escape');
    await page.waitForTimeout(100);
  }

  // chip token
  {
    const cells = [];
    const pane = page.locator('.pane').first();
    const chip = pane.locator('button.gchip').first();
    const f1 = path.join(tmpDir, 'chip_rest.png');
    await shot(chip, f1);
    cells.push({ file: f1, label: 'rest' });
    await chip.hover({ force: true });
    await page.waitForTimeout(80);
    const f2 = path.join(tmpDir, 'chip_hover.png');
    await shot(chip, f2);
    cells.push({ file: f2, label: 'hover' });
    await chip.focus().catch(() => {});
    await page.waitForTimeout(80);
    const f3 = path.join(tmpDir, 'chip_focus.png');
    await shot(chip, f3);
    cells.push({ file: f3, label: 'focus-visible' });
    const gridFile = await buildGrid(page, 'chip_token', viewport.name, cells);
    manifest.push({ component: 'chip token', viewport: viewport.name, grid: path.relative(ROOT, gridFile) });
  }

  fs.rmSync(tmpDir, { recursive: true, force: true });
  await ctx.close();
}

async function main() {
  fs.mkdirSync(PAGES_DIR, { recursive: true });
  fs.mkdirSync(COMPONENTS_DIR, { recursive: true });
  const browser = await launchBrowser();

  const pageResults = [];
  for (const [stateKey, state] of Object.entries(STATES)) {
    if (ONLY && !ONLY.includes(stateKey)) continue;
    for (const viewport of VIEWPORTS) {
      process.stdout.write(`page ${stateKey} @ ${viewport.name} ... `);
      await capturePageState(browser, stateKey, state, viewport, pageResults);
      console.log(pageResults[pageResults.length - 1].status);
    }
  }
  fs.writeFileSync(path.join(ROOT, 'capture_log.json'), JSON.stringify(pageResults, null, 2));

  const componentManifest = [];
  for (const viewport of VIEWPORTS) {
    process.stdout.write(`components @ ${viewport.name} ... `);
    await captureComponentsAtViewport(browser, viewport, componentManifest);
    console.log('ok');
  }
  fs.writeFileSync(path.join(ROOT, 'component_matrix.json'), JSON.stringify(componentManifest, null, 2));

  await browser.close();
  const failed = pageResults.filter((r) => r.status !== 'ok' && r.status !== 'unreachable');
  console.log(`\n${pageResults.length} page captures, ${failed.length} non-ok. ${componentManifest.length} component grids.`);
  if (failed.length) console.log(JSON.stringify(failed, null, 2));
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
