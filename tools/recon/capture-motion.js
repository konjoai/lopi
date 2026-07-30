// Step 3 — motion=on deliverables: a 20-frame @100ms strip during S4's
// running prompt, and the two 30-second videos (S4 streaming; a working
// session on the Loop Stacks page exercising popover/config/alias/chip).
'use strict';

const fs = require('fs');
const path = require('path');
const { launchBrowser, newContext, BASE_URL } = require('./lib/browser');
const { installRestMocks, installWsReplay, sendSequence, installTaskCreateMock } = require('./lib/mock');
const { seedState, goalToTaskIdMap } = require('./lib/seed');
const { STATES } = require('./fixtures/states');

const SHOTS_DIR = path.join(__dirname, '..', '..', 'recon', 'shots');
const FRAMES_DIR = path.join(SHOTS_DIR, 'motion_frames_s4');
const VIDEOS_DIR = path.join(__dirname, '..', '..', 'recon', 'videos');

async function frameStrip(browser) {
  fs.mkdirSync(FRAMES_DIR, { recursive: true });
  const { ctx, page } = await newContext(browser, { viewport: { width: 1440, height: 900 }, motion: 'on' });
  const state = STATES.S4_streaming_now;
  await installRestMocks(page, state.rest || {});
  const ws = installWsReplay(page);
  await installTaskCreateMock(page, goalToTaskIdMap(state.seedCard));
  await ws.armed;
  await page.goto(`${BASE_URL}/stacks`, { waitUntil: 'load' });
  await page.waitForTimeout(300);
  await seedState(page, state.seedCard);
  const route = await Promise.race([ws.connected, new Promise((r) => setTimeout(() => r(null), 2000))]);
  if (route) sendSequence(page, route, state.ws).catch(() => {});

  await page.waitForTimeout(1200); // let streaming get underway before sampling
  for (let i = 0; i < 20; i++) {
    await page.screenshot({ path: path.join(FRAMES_DIR, `frame_${String(i).padStart(2, '0')}.png`) });
    await page.waitForTimeout(100);
  }
  await ctx.close();
  console.log(`wrote 20 frames to ${FRAMES_DIR}`);
}

async function videoS4(browser) {
  fs.mkdirSync(VIDEOS_DIR, { recursive: true });
  const { ctx, page } = await newContext(browser, {
    viewport: { width: 1440, height: 900 },
    motion: 'on',
    recordVideo: { dir: VIDEOS_DIR, size: { width: 1440, height: 900 } }
  });
  const state = STATES.S4_streaming_now;
  await installRestMocks(page, state.rest || {});
  const ws = installWsReplay(page);
  await installTaskCreateMock(page, goalToTaskIdMap(state.seedCard));
  await ws.armed;
  await page.goto(`${BASE_URL}/stacks`, { waitUntil: 'load' });
  await page.waitForTimeout(300);
  await seedState(page, state.seedCard);
  const route = await Promise.race([ws.connected, new Promise((r) => setTimeout(() => r(null), 2000))]);
  if (route) await sendSequence(page, route, state.ws);
  await page.waitForTimeout(30000 - 5000); // pad to ~30s total wall clock
  const videoPath = await page.video().path();
  await ctx.close();
  const finalPath = path.join(VIDEOS_DIR, 's4_streaming.webm');
  fs.renameSync(videoPath, finalPath);
  console.log(`wrote ${finalPath}`);
}

async function videoWorkingSession(browser) {
  fs.mkdirSync(VIDEOS_DIR, { recursive: true });
  const { ctx, page } = await newContext(browser, {
    viewport: { width: 1440, height: 900 },
    motion: 'on',
    recordVideo: { dir: VIDEOS_DIR, size: { width: 1440, height: 900 } }
  });
  const state = STATES.S13_loop_stacks_populated;
  await installRestMocks(page, state.rest || {});
  const ws = installWsReplay(page);
  await installTaskCreateMock(page, goalToTaskIdMap(state.seedCard));
  await ws.armed;
  await page.goto(`${BASE_URL}/stacks`, { waitUntil: 'load' });
  await page.waitForTimeout(500);
  await seedState(page, state.seedCard);
  const route = await Promise.race([ws.connected, new Promise((r) => setTimeout(() => r(null), 2000))]);
  if (route) await sendSequence(page, route, state.ws);
  await page.waitForTimeout(1000);

  const pane = page.locator('.pane').first();

  // Open a popover.
  await pane.locator('button[title="schedule the stack"]').first().click({ force: true });
  await page.waitForTimeout(1200);
  await page.keyboard.press('Escape');
  await page.waitForTimeout(600);

  // Change a config.
  await pane.locator('button[title="stack default config"]').first().click({ force: true });
  await page.waitForTimeout(1000);
  const modelRow = page.locator('.cfgrow.model select, .cfgrow.model button').first();
  await modelRow.click({ force: true }).catch(() => {});
  await page.waitForTimeout(800);
  await page.keyboard.press('Escape');
  await page.waitForTimeout(600);

  // Edit an alias / add + remove a chip via the composer grammar.
  const composer = pane.locator('.chipinput').first();
  await composer.click();
  await page.keyboard.type(' :delta');
  await page.waitForTimeout(700);
  await page.keyboard.press('Backspace');
  await page.keyboard.press('Backspace');
  await page.keyboard.press('Backspace');
  await page.keyboard.press('Backspace');
  await page.keyboard.press('Backspace');
  await page.keyboard.press('Backspace');
  await page.waitForTimeout(600);

  await page.waitForTimeout(30000 - 6500); // pad to ~30s total wall clock
  const videoPath = await page.video().path();
  await ctx.close();
  const finalPath = path.join(VIDEOS_DIR, 'working_session_loop_stacks.webm');
  fs.renameSync(videoPath, finalPath);
  console.log(`wrote ${finalPath}`);
}

async function main() {
  const browser = await launchBrowser();
  const which = process.argv[2];
  if (!which || which === 'frames') await frameStrip(browser);
  if (!which || which === 's4') await videoS4(browser);
  if (!which || which === 'session') await videoWorkingSession(browser);
  await browser.close();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
