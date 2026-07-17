import { test, expect } from '@playwright/test';

/**
 * Popover-Fix-1 — regression test for the exact bug KT2 found: the schedule
 * popover grows when "run on a schedule" is toggled on (a cron builder
 * mounts inside it), and at a short viewport the old `computePosition()`
 * never re-ran after that growth, so the popover overflowed the bottom of
 * the window. The fix is a `ResizeObserver` on the popover element
 * (`web/src/lib/components/stacks/Popover.svelte`). This test asserts the
 * popover's bounding box stays fully inside the viewport both before and
 * after the content grows.
 */

test('stack schedule popover stays fully on-screen at a short viewport after content grows', async ({
  page
}) => {
  await page.setViewportSize({ width: 1280, height: 700 });
  await page.goto('/stacks');

  const input = page.locator('input.goalinput').first();
  await input.click();
  await input.fill('popover viewport check');
  await page.getByRole('button', { name: 'add', exact: true }).first().click();

  await page.getByRole('button', { name: 'stack controls' }).first().click();
  await page.getByRole('button', { name: 'schedule the stack' }).first().click();

  const popover = page.locator('.pop.sched').first();
  await expect(popover).toBeVisible();

  const before = await popover.boundingBox();
  expect(before).not.toBeNull();
  expect(before!.y + before!.height).toBeLessThanOrEqual(700);

  // Toggling the schedule on mounts the cron builder — the exact content
  // growth that triggered the pre-fix overflow.
  await popover.locator('button').first().click();
  await expect(popover.getByText('next runs:')).toBeVisible();

  const after = await popover.boundingBox();
  expect(after).not.toBeNull();
  expect(after!.y).toBeGreaterThanOrEqual(0);
  expect(after!.y + after!.height).toBeLessThanOrEqual(700);
});
