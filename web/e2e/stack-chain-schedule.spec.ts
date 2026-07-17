import { test, expect } from '@playwright/test';

/**
 * Stack-Chain-1 — end-to-end coverage for the stack control dock's "schedule
 * the entire stack" toggle. Mocks `/api/schedule-chains` (the backend itself
 * has its own coverage — `crates/lopi-orchestrator/tests/chain_schedule_resume.rs`
 * and the handler tests in `crates/lopi-ui/src/web/schedule_chains_tests.rs`)
 * and asserts the CLIENT wires every card into one request, in execution
 * order — the exact bug this sprint fixed (pre-sprint, only the first card
 * was ever wired).
 */

async function addCard(page: import('@playwright/test').Page, goal: string) {
  const input = page.locator('input.goalinput').first();
  await input.click();
  await input.fill(goal);
  await page.getByRole('button', { name: 'add', exact: true }).first().click();
}

test('schedule-the-stack toggle wires every card into one chain request', async ({ page }) => {
  const requests: Array<{ url: string; body: unknown }> = [];
  await page.route('**/api/schedule-chains', async (route) => {
    const body = route.request().postDataJSON();
    requests.push({ url: route.request().url(), body });
    await route.fulfill({
      status: 201,
      contentType: 'application/json',
      body: JSON.stringify({ id: 'chain-e2e-1', ...body, enabled: true, next_runs: [], last_run: null })
    });
  });

  await page.goto('/stacks');
  await addCard(page, 'e2e step one');
  await addCard(page, 'e2e step two');

  await page.getByRole('button', { name: 'stack controls' }).first().click();
  await page.getByRole('button', { name: 'schedule the stack' }).first().click();

  const toggle = page.locator('.pop.sched button').first();
  await toggle.click();

  await expect.poll(() => requests.length, { timeout: 5000 }).toBeGreaterThan(0);
  const body = requests[0].body as { steps: Array<{ goal: string }> };
  expect(body.steps.map((s) => s.goal)).toEqual(['e2e step one', 'e2e step two']);
});
