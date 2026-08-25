// UI-driven, deterministic pane/card seeding for Sprint U-0 fixtures.
// Stacks are 100% client-side (web/src/lib/stores/stackRun.ts's own doc
// comment) — there is no server "stack" concept to seed via the API, so the
// only faithful way to reach a given composer/card state is to drive the
// same composer + run-stack interactions a real user would.
'use strict';

async function addPane(page) {
  await page.locator('header button, [class*="topbar"] button').filter({ hasText: '+' }).last().click();
}

async function typeGoalAndAdd(page, paneIndex, goal) {
  const pane = page.locator('.pane').nth(paneIndex);
  const composer = pane.locator('.chipinput').first();
  await composer.click();
  await page.keyboard.type(goal);
  await page.waitForTimeout(80);
  await pane.getByRole('button', { name: 'add', exact: true }).first().click();
  await page.waitForTimeout(150);
}

async function runPane(page, paneIndex) {
  const pane = page.locator('.pane').nth(paneIndex);
  await pane.getByRole('button', { name: 'stack controls' }).first().click({ force: true });
  await page.waitForTimeout(120);
  await pane.getByRole('button', { name: 'run stack', exact: true }).first().click({ force: true });
  await page.waitForTimeout(150);
  // Close the dock again so it doesn't occlude the page-state screenshot.
  await pane.getByRole('button', { name: 'stack controls' }).first().click({ force: true }).catch(() => {});
  await page.waitForTimeout(100);
}

/**
 * Seed a state per fixtures/states.js's `seedCard` contract:
 *  - null                      → nothing to seed (page loads empty/default)
 *  - {goal, taskId}             → one card, one pane, run it
 *  - {goal, taskId, extraCards} → one pane, multiple chained cards (S13
 *                                 composer density); only the first is run
 *  - [{goal, taskId}, ...]      → one pane per entry, each run independently
 *                                 (S3 concurrent, S12 all-finished)
 *  - [{goal,taskId}, {goal,taskId:null,leaveQueued:true}] → S11: two cards in
 *    ONE pane, only the first is run — the second stays 'idle'/'queued'
 *    behind it in chain order.
 * Returns the {goal → taskId} map the caller must feed to
 * installTaskCreateMock BEFORE navigation (so the POST mock is armed before
 * any request fires).
 */
function goalToTaskIdMap(seedCard) {
  const map = {};
  if (!seedCard) return map;
  const entries = Array.isArray(seedCard) ? seedCard : [seedCard];
  for (const e of entries) {
    if (e.taskId) map[e.goal] = e.taskId;
    if (e.extraCards) {
      // extraCards are same-pane chain members with no task id of their own
      // in these fixtures (composer-density only) — nothing to map.
    }
  }
  return map;
}

async function seedState(page, seedCard) {
  if (!seedCard) return;

  if (Array.isArray(seedCard) && seedCard.length === 2 && seedCard[1].leaveQueued) {
    // S11: two cards, one pane, only card 1 runs.
    await typeGoalAndAdd(page, 0, seedCard[0].goal);
    await typeGoalAndAdd(page, 0, seedCard[1].goal);
    await runPane(page, 0);
    return;
  }

  if (Array.isArray(seedCard)) {
    // One pane per entry (S3, S12) — default page ships 2 panes; add more.
    for (let i = 2; i < seedCard.length; i++) await addPane(page);
    for (let i = 0; i < seedCard.length; i++) {
      await typeGoalAndAdd(page, i, seedCard[i].goal);
      await runPane(page, i);
    }
    return;
  }

  // Single object: one pane, optionally extraCards chained after it (S13).
  await typeGoalAndAdd(page, 0, seedCard.goal);
  if (seedCard.extraCards) {
    for (const goal of seedCard.extraCards) await typeGoalAndAdd(page, 0, goal);
  }
  if (seedCard.taskId) await runPane(page, 0);
}

module.exports = { addPane, typeGoalAndAdd, runPane, seedState, goalToTaskIdMap };
