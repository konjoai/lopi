import { defineConfig, devices } from '@playwright/test';

/**
 * Stack-Chain-1 / Popover-Fix-1 — first Playwright config in this repo
 * (logged as a one-way-door dependency addition, see `LEDGER.md`).
 *
 * `webServer` starts the Vite dev server only — the backend is mocked per
 * spec via `page.route()` rather than requiring a live `lopi sail` process,
 * so `npm run test:e2e` is self-contained in CI with no Rust build step.
 */
export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  reporter: 'list',
  use: {
    baseURL: 'http://localhost:5173',
    trace: 'on-first-retry'
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: {
    command: 'npm run dev',
    url: 'http://localhost:5173',
    reuseExistingServer: !process.env.CI,
    timeout: 30_000
  }
});
