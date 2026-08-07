// Census F — keyboard, focus, and layering.
'use strict';

const fs = require('fs');
const path = require('path');
const { launchBrowser, newContext, BASE_URL } = require('./lib/browser');
const { installRestMocks, installWsReplay, sendSequence, installTaskCreateMock } = require('./lib/mock');
const { seedState, goalToTaskIdMap } = require('./lib/seed');
const { STATES } = require('./fixtures/states');
const { parseColor, toHex, contrastRatio } = require('./lib/color');

const OUT_INTERACTION = path.join(__dirname, '..', '..', 'recon', 'census_interaction.json');
const OUT_KEYBOARD = path.join(__dirname, '..', '..', 'recon', 'census_keyboard.json');

const DESCRIBE_ACTIVE = () => {
  const el = document.activeElement;
  if (!el || el === document.body) return null;
  const rect = el.getBoundingClientRect();
  const cs = getComputedStyle(el);
  return {
    selector: `${el.tagName.toLowerCase()}${el.className ? '.' + String(el.className).split(' ')[0] : ''}`,
    title: el.getAttribute('title') || el.getAttribute('aria-label') || el.textContent?.trim().slice(0, 24) || '',
    x: Math.round(rect.x),
    y: Math.round(rect.y),
    outlineColor: cs.outlineColor,
    outlineWidth: cs.outlineWidth,
    outlineStyle: cs.outlineStyle,
    boxShadow: cs.boxShadow === 'none' ? null : cs.boxShadow,
    backgroundColor: cs.backgroundColor
  };
};

const Z_INDEX_INVENTORY = () => {
  const out = [];
  for (const el of document.querySelectorAll('body *')) {
    const cs = getComputedStyle(el);
    if (cs.zIndex !== 'auto' && cs.position !== 'static') {
      out.push({
        selector: `${el.tagName.toLowerCase()}${el.className ? '.' + String(el.className).split(' ')[0] : ''}`,
        zIndex: cs.zIndex,
        position: cs.position
      });
    }
  }
  return out;
};

async function main() {
  const browser = await launchBrowser();
  const { ctx, page } = await newContext(browser, { viewport: { width: 1440, height: 900 }, motion: 'off' });

  const s13 = STATES.S13_loop_stacks_populated;
  await installRestMocks(page, s13.rest || {});
  const ws = installWsReplay(page);
  await installTaskCreateMock(page, goalToTaskIdMap(s13.seedCard));
  await ws.armed;
  await page.goto(`${BASE_URL}/stacks`, { waitUntil: 'load' });
  await page.waitForTimeout(300);
  await seedState(page, s13.seedCard);
  const route = await Promise.race([ws.connected, new Promise((r) => setTimeout(() => r(null), 2000))]);
  if (route) await sendSequence(page, route, s13.ws);
  await page.waitForTimeout(300);

  // 1. Tab order — walk the first 25 stops from a clean start (body focus).
  await page.evaluate(() => document.body.focus());
  const tabOrder = [];
  for (let i = 0; i < 25; i++) {
    await page.keyboard.press('Tab');
    const info = await page.evaluate(DESCRIBE_ACTIVE);
    if (!info) break;
    tabOrder.push({ step: i + 1, ...info });
  }
  // Visual-order check: does y (then x) monotonically non-decrease, allowing
  // for same-row left-to-right groups? Flag any backward jump > 40px in y
  // that isn't explained by a new row starting further up (i.e. a real
  // out-of-order jump).
  const outOfOrder = [];
  for (let i = 1; i < tabOrder.length; i++) {
    const prev = tabOrder[i - 1];
    const cur = tabOrder[i];
    if (cur.y < prev.y - 40 && cur.x < prev.x - 40) {
      outOfOrder.push({ from: prev.selector, to: cur.selector, step: cur.step });
    }
  }

  // 2. Focus indicator contrast for each tab stop that has a visible outline
  //    or box-shadow ring.
  const focusContrast = [];
  for (const stop of tabOrder) {
    const outlineRgba = parseColor(stop.outlineColor);
    const bgRgba = parseColor(stop.backgroundColor);
    const hasVisibleOutline = outlineRgba && stop.outlineStyle !== 'none' && parseFloat(stop.outlineWidth) > 0;
    focusContrast.push({
      selector: stop.selector,
      title: stop.title,
      hasVisibleOutline,
      outlineWidth: stop.outlineWidth,
      boxShadow: stop.boxShadow,
      ratioVsOwnBackground:
        hasVisibleOutline && bgRgba ? Math.round(contrastRatio(outlineRgba, bgRgba) * 100) / 100 : null
    });
  }
  const noVisibleFocusIndicator = focusContrast.filter((f) => !f.hasVisibleOutline && !f.boxShadow);

  // 3. Popover focus management: open the schedule popover, check focus
  //    moved in, Tab within it, Escape, check focus returned to trigger.
  const schedBtn = page.locator('button[title="schedule the stack"]').first();
  await schedBtn.click({ force: true });
  await page.waitForTimeout(200);
  const focusAfterOpen = await page.evaluate(DESCRIBE_ACTIVE);
  const focusMovedIntoPopover = await page.evaluate(() => {
    const el = document.activeElement;
    return !!(el && el.closest('.pop'));
  });
  await page.keyboard.press('Tab');
  const focusAfterTabInside = await page.evaluate(DESCRIBE_ACTIVE);
  const stillInsidePopover = await page.evaluate(() => {
    const el = document.activeElement;
    return !!(el && el.closest('.pop'));
  });
  await page.keyboard.press('Escape');
  await page.waitForTimeout(150);
  const popoverClosedByEscape = (await page.locator('.pop.sched').count()) === 0;
  const focusAfterEscape = await page.evaluate(DESCRIBE_ACTIVE);
  const focusReturnedToTrigger = await schedBtn.evaluate((el) => el === document.activeElement).catch(() => false);

  // 4. Stacking/z-index inventory.
  const zIndexes = await page.evaluate(Z_INDEX_INVENTORY);

  const keyboardResult = {
    tab_order: tabOrder,
    out_of_visual_order_jumps: outOfOrder,
    focus_indicator_contrast: focusContrast,
    elements_with_no_visible_focus_indicator: noVisibleFocusIndicator,
    popover_focus_management: {
      focus_moved_into_popover_on_open: focusMovedIntoPopover,
      focus_after_open: focusAfterOpen,
      tab_stayed_inside_popover: stillInsidePopover,
      focus_after_tab_inside: focusAfterTabInside,
      escape_closed_popover: popoverClosedByEscape,
      focus_returned_to_trigger_after_escape: focusReturnedToTrigger,
      focus_after_escape: focusAfterEscape
    }
  };

  const interactionResult = {
    z_index_inventory: zIndexes,
    distinct_z_index_values: [...new Set(zIndexes.map((z) => z.zIndex))].sort((a, b) => Number(a) - Number(b))
  };

  fs.writeFileSync(OUT_KEYBOARD, JSON.stringify(keyboardResult, null, 2));
  fs.writeFileSync(OUT_INTERACTION, JSON.stringify(interactionResult, null, 2));
  console.log(`wrote ${OUT_KEYBOARD} and ${OUT_INTERACTION}`);
  console.log(`tab stops captured: ${tabOrder.length}, out-of-order jumps: ${outOfOrder.length}`);
  console.log(`no visible focus indicator: ${noVisibleFocusIndicator.length}/${tabOrder.length}`);
  console.log(`focus moved into popover on open: ${focusMovedIntoPopover}, escape closed: ${popoverClosedByEscape}, focus returned to trigger: ${focusReturnedToTrigger}`);
  console.log(`distinct z-index values: ${interactionResult.distinct_z_index_values.join(', ')}`);

  await ctx.close();
  await browser.close();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
