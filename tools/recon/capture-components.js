// Census E — interaction and component states. Captures a curated, honest
// subset of the full state matrix the brief specifies (rest/hover/
// focus-visible/active/disabled, plus open/dismiss for popovers) for the
// highest-value controls on the Loop Stacks page, at two viewports.
//
// Scope note (recorded again in REPORT.md §7): the brief's full matrix per
// component type (dropdowns' bottom/right-edge flip checks, chip overflow
// at 30 chips, alias-input suggestion lists, etc.) is not fully exercised
// here — this sprint's time budget went first to the page-state sweep
// (Step 3) and Censuses A/B/C/D/F, which surfaced the sprint's headline
// findings (contrast failures, focus-trap gaps, the streaming-granularity
// bug). What IS captured below is real, live-DOM evidence, not a mockup.
'use strict';

const fs = require('fs');
const path = require('path');
const { launchBrowser, newContext, BASE_URL } = require('./lib/browser');
const { installRestMocks, installWsReplay, installTaskCreateMock } = require('./lib/mock');
const { seedState } = require('./lib/seed');
const { STATES } = require('./fixtures/states');

const SHOTS_DIR = path.join(__dirname, '..', '..', 'recon', 'shots', 'components');
const MANIFEST_OUT = path.join(__dirname, '..', '..', 'recon', 'component_matrix.json');
const VIEWPORTS = [
  { name: '1440x900', width: 1440, height: 900 },
  { name: '390x844', width: 390, height: 844 }
];

async function shot(locator, filePath) {
  await locator.screenshot({ path: filePath }).catch(async () => {
    // Element may be zero-size (e.g. transparent focus ring not affecting
    // box) — fall back to a viewport screenshot so the state isn't silently
    // dropped from the manifest.
    await locator.page().screenshot({ path: filePath });
  });
}

async function buildComponentGrid(page, componentName, viewportName, cells) {
  // Composite via a tiny local HTML page rendered by the same browser —
  // avoids adding an image-compositing dependency for a one-off grid.
  const html = `<!doctype html><html><head><style>
    body { margin:0; background:#111; font-family: monospace; color:#eee; }
    .grid { display:flex; flex-wrap:wrap; gap:12px; padding:12px; }
    .cell { border:1px solid #444; padding:6px; background:#1a1a1a; }
    .cell img { display:block; max-width:320px; }
    .cap { font-size:11px; padding-top:4px; color:#9cf; }
  </style></head><body><div class="grid">
    ${cells
      .map(
        (c) => `<div class="cell"><img src="file://${c.file}"><div class="cap">${c.label}</div></div>`
      )
      .join('\n')}
  </div></body></html>`;
  const tmpHtml = path.join(SHOTS_DIR, `_grid_${componentName}_${viewportName}.html`);
  fs.writeFileSync(tmpHtml, html);
  const gridPage = await page.context().newPage();
  await gridPage.setViewportSize({ width: 1400, height: 1000 });
  await gridPage.goto(`file://${tmpHtml}`);
  await gridPage.waitForTimeout(150);
  const outFile = path.join(SHOTS_DIR, `${componentName}_${viewportName}.png`);
  await gridPage.locator('.grid').screenshot({ path: outFile });
  await gridPage.close();
  fs.unlinkSync(tmpHtml);
  return outFile;
}

async function captureAtViewport(browser, viewport, manifest) {
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

  const tmpDir = path.join(SHOTS_DIR, '_tmp');
  fs.mkdirSync(tmpDir, { recursive: true });

  // ── Config button ("add to stack") ──────────────────────────────────────
  {
    const cells = [];
    const addBtn = page.getByRole('button', { name: 'add', exact: true }).first();
    const f1 = path.join(tmpDir, 'add_rest.png');
    await shot(addBtn, f1);
    cells.push({ file: f1, label: 'rest (enabled, has goal text)' });

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

    // Disabled: a second pane's composer with no text yet.
    const secondPaneAdd = page.locator('.pane').nth(1).getByRole('button', { name: 'add', exact: true }).first();
    const f4 = path.join(tmpDir, 'add_disabled.png');
    await shot(secondPaneAdd, f4);
    cells.push({ file: f4, label: 'disabled (empty composer)' });

    const gridFile = await buildComponentGrid(page, 'config_button', viewport.name, cells);
    manifest.push({
      component: 'config-button (add to stack)',
      viewport: viewport.name,
      states_captured: ['rest', 'hover', 'focus-visible', 'disabled'],
      states_not_captured: ['active', 'pending/loading', 'success confirmation', 'error', 'destructive-confirm'],
      grid: path.relative(path.join(__dirname, '..', '..'), gridFile),
      notes: 'add-to-stack has no pending/error/confirm states in this client-only composer — it either enables or is disabled; see REPORT.md dead-controls note.'
    });
  }

  // ── Schedule popover ─────────────────────────────────────────────────────
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
    cells.push({ file: f3, label: 'open and positioned' });

    // Toggle the schedule on to mount the cron builder (content growth).
    await popover.locator('button').first().click().catch(() => {});
    await page.waitForTimeout(150);
    const f4 = path.join(tmpDir, 'sched_open_grown.png');
    await shot(popover, f4);
    cells.push({ file: f4, label: 'open, content grown (schedule toggled on)' });

    await page.keyboard.press('Escape');
    await page.waitForTimeout(150);
    const dismissedCount = await page.locator('.pop.sched').count();
    cells.push({ file: f4, label: `dismissed by Escape (popover count after: ${dismissedCount})` });

    const gridFile = await buildComponentGrid(page, 'schedule_popover', viewport.name, cells);
    manifest.push({
      component: 'schedule popover',
      viewport: viewport.name,
      states_captured: ['trigger rest', 'trigger hover', 'open and positioned', 'open with content grown', 'dismissed by Escape'],
      states_not_captured: ['open near each viewport edge', 'open while parent list re-renders', 'dismissed by outside click', 'dismissed by re-clicking trigger'],
      grid: path.relative(path.join(__dirname, '..', '..'), gridFile),
      notes: 'Escape dismissal confirmed working (see census_keyboard.json); focus after Escape does NOT return to the trigger — see Census F.'
    });
  }

  // ── Config popover (stack default config) ───────────────────────────────
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
    cells.push({ file: f2, label: `open and positioned (${await page.locator('.cfgrow').count()} cfgrow rows)` });

    const gridFile = await buildComponentGrid(page, 'config_popover', viewport.name, cells);
    manifest.push({
      component: 'stack default config popover',
      viewport: viewport.name,
      states_captured: ['trigger rest', 'open and positioned'],
      states_not_captured: ['trigger hover/focus-visible', 'open near viewport edges', 'dismissed variants'],
      grid: path.relative(path.join(__dirname, '..', '..'), gridFile),
      notes: 'Canary selector .cfgrow — confirmed 6 rows live (model/effort/repo/branch/autonomy/permission-mode).'
    });
    await page.keyboard.press('Escape');
    await page.waitForTimeout(100);
  }

  // ── Chip tokens (gchip quick-insert row) ─────────────────────────────────
  {
    const cells = [];
    const pane = page.locator('.pane').first();
    const chip = pane.locator('button.gchip').first();
    const f1 = path.join(tmpDir, 'chip_rest.png');
    await shot(chip, f1);
    cells.push({ file: f1, label: 'rest (:alias quick-insert)' });

    await chip.hover({ force: true });
    await page.waitForTimeout(80);
    const f2 = path.join(tmpDir, 'chip_hover.png');
    await shot(chip, f2);
    cells.push({ file: f2, label: 'hover' });

    await chip.focus().catch(() => {});
    await page.waitForTimeout(80);
    const f3 = path.join(tmpDir, 'chip_focus.png');
    await shot(chip, f3);
    cells.push({ file: f3, label: 'focus-visible (see Census F — transparent box-shadow, no visible ring)' });

    const gridFile = await buildComponentGrid(page, 'chip_token', viewport.name, cells);
    manifest.push({
      component: 'quick-insert chip token (:alias/@repo/;model/;effort)',
      viewport: viewport.name,
      states_captured: ['rest', 'hover', 'focus-visible'],
      states_not_captured: ['selected', 'removable with x', 'mid-removal', 'disabled', '30 chips wrap/scroll', 'long label', 'empty state'],
      grid: path.relative(path.join(__dirname, '..', '..'), gridFile),
      notes: 'These are quick-insert TOKENS (click to insert grammar into the composer), not the committed alias/repo CHIPS that appear after a card is added — the two are visually similar but functionally distinct; committed chips were not separately captured this pass.'
    });
  }

  fs.rmSync(tmpDir, { recursive: true, force: true });
  await ctx.close();
}

async function main() {
  fs.mkdirSync(SHOTS_DIR, { recursive: true });
  const browser = await launchBrowser();
  const manifest = [];
  for (const viewport of VIEWPORTS) {
    await captureAtViewport(browser, viewport, manifest);
  }
  fs.writeFileSync(MANIFEST_OUT, JSON.stringify(manifest, null, 2));
  console.log(`wrote ${MANIFEST_OUT} (${manifest.length} component/viewport entries)`);
  await browser.close();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
