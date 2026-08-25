// Shared browser/page setup for Sprint U-0 recon capture scripts.
// Dev-only tooling — never imported by the lopi workspace (tools/recon/ has
// its own package.json and node_modules).
'use strict';

const { chromium } = require('playwright');

const CHROMIUM_PATH = '/opt/pw-browsers/chromium';
const BASE_URL = 'http://localhost:5173';

// Fixed instant so every relative timestamp ("3m ago", clocks, etc.) renders
// identically across runs. 2026-07-30T12:00:00Z — mid-day UTC, arbitrary but
// fixed, matches the RECON_REF checkout date.
const FROZEN_NOW_MS = Date.parse('2026-07-30T12:00:00.000Z');

const MOTION_OFF_CSS = `
  *, *::before, *::after {
    animation: none !important;
    transition: none !important;
    caret-color: transparent !important;
  }
`;

// Force a fixed font stack — the page loads Inter/JetBrains Mono from
// fonts.googleapis.com, which is unreachable from this sandbox (confirmed:
// net::ERR_CONNECTION_RESET during pre-flight) and would otherwise fall back
// unpredictably. Every capture in this sprint therefore renders in the
// system font stack, not the production Inter/JetBrains Mono webfonts — see
// REPORT.md section 2.
const FONT_OVERRIDE_CSS = `
  html, body, * {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif !important;
  }
  code, pre, .mono, [class*="mono"] {
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace !important;
  }
`;

async function launchBrowser() {
  return chromium.launch({ executablePath: CHROMIUM_PATH, headless: true });
}

/**
 * New browser context with the clock frozen at FROZEN_NOW_MS, dsf=2, and
 * (optionally) motion disabled. `viewport` is {width, height}.
 */
async function newContext(browser, { viewport, motion = 'off', recordVideo = undefined }) {
  const ctx = await browser.newContext({
    viewport,
    deviceScaleFactor: 2,
    ...(recordVideo ? { recordVideo } : {})
  });
  await ctx.addInitScript(`{
    const FROZEN = ${FROZEN_NOW_MS};
    const RealDate = Date;
    class FrozenDate extends RealDate {
      constructor(...args) {
        if (args.length === 0) { super(FROZEN); return; }
        super(...args);
      }
      static now() { return FROZEN; }
    }
    window.Date = FrozenDate;
  }`);
  const page = await ctx.newPage();
  await page.addStyleTag({ content: FONT_OVERRIDE_CSS });
  if (motion === 'off') {
    await page.addStyleTag({ content: MOTION_OFF_CSS });
  }
  return { ctx, page };
}

module.exports = { launchBrowser, newContext, BASE_URL, FROZEN_NOW_MS, CHROMIUM_PATH };
