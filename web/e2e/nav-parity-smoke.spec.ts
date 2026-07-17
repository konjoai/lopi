import { test, expect } from '@playwright/test';

/**
 * Parity-Audit-1 — smoke test for every web nav section enumerated in
 * `docs/ops/PARITY_AUDIT_2026-07-16.md` §1. Each route must render without
 * a client-side exception; this is deliberately shallow (load + no console
 * error), not a feature test — the point is to catch a broken route before
 * a parity claim is made about it.
 */

const ROUTES = ['/stacks', '/loop', '/budget', '/schedules', '/overview', '/config'];

for (const route of ROUTES) {
  test(`${route} loads without a client-side error`, async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (err) => errors.push(err.message));

    await page.goto(route);
    await expect(page.locator('body')).toBeVisible();
    expect(errors).toEqual([]);
  });
}
