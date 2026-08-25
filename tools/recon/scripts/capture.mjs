// Sprint U-0 dashboard recon — screenshot + census capture driver.
//
// Throwaway tooling (not part of the lopi workspace or its build). Spawns
// the dev-only fixture-server (tools/recon/fixture-server) once per state,
// drives the REAL embedded SvelteKit dashboard with Playwright through its
// real composer UI (panes/cards are pure client-side session state — see
// web/src/lib/stores/stack.ts — never rehydrated from the server, so a
// state only exists on screen if a real card was created via the real
// POST /api/tasks flow), then calls the fixture-server's recon-only
// /recon/pump control route to replay a deterministic AgentEvent sequence
// for that real task id.
//
// Writes:
//   - recon/shots/<state>_<viewport>_off.png                (motion=off still)
//   - recon/shots/<state>_<viewport>_on_frame<NN>.png        (20-frame strip, motion=on)
//   - recon/shots/S4_streaming_30s.webm                      (one video, S4 only)
//   - recon/raw/<state>.json                                  (per-state census raw data)
//
// Determinism: fixed viewports, DPR 2, frozen clock, blocked webfont
// requests + fixed font-stack override, motion=off CSS override for stills.
// Card/task creation happens for real on each run (server assigns a fresh
// UUID and a real creation timestamp) — content and event timing are fixed,
// but the literal task id and the exact "created Ns ago" second are not
// byte-reproducible run over run. See recon/LEDGER.md.

import { chromium } from '@playwright/test';
import { spawn } from 'node:child_process';
import { mkdir, writeFile, rename } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, '..', '..', '..');
const SHOTS_DIR = path.join(REPO_ROOT, 'recon', 'shots');
const RAW_DIR = path.join(REPO_ROOT, 'recon', 'raw');
const FIXTURE_BIN = path.join(REPO_ROOT, 'tools', 'recon', 'fixture-server', 'target', 'debug', 'fixture-server');
const CHROME_PATH = '/opt/pw-browsers/chromium-1194/chrome-linux/chrome';

const VIEWPORTS = [
  { name: '1920x1080', width: 1920, height: 1080 },
  { name: '1440x900', width: 1440, height: 900 },
  { name: '1280x800', width: 1280, height: 800 },
  { name: '768x1024', width: 768, height: 1024 },
  { name: '390x844', width: 390, height: 844 }
];
const DPR = 2;
const FROZEN_CLOCK = '2026-07-30T00:00:00.000Z';

const FIXED_FONT_CSS = `
*, *::before, *::after {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif !important;
}
code, pre, .font-mono, [class*="mono"] {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace !important;
}
`;
const MOTION_OFF_CSS = `* { animation: none !important; transition: none !important; }`;

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function waitForHealth(port, timeoutMs = 15000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const res = await fetch(`http://127.0.0.1:${port}/api/health`);
      if (res.ok) return true;
    } catch {
      /* not up yet */
    }
    await sleep(100);
  }
  throw new Error(`fixture-server on :${port} never became healthy`);
}

function spawnFixture(port) {
  const dbPath = `/tmp/lopi-recon-${port}.db`;
  const child = spawn(FIXTURE_BIN, ['--port', String(port), '--db', dbPath], { stdio: ['ignore', 'pipe', 'pipe'] });
  child.stdout.on('data', () => {});
  child.stderr.on('data', () => {});
  return child;
}

async function pump(port, taskId, scenario) {
  if (!taskId) {
    throw new Error(`pump(${scenario}) called with no task id — a waitForIds() call is missing upstream`);
  }
  const res = await fetch(`http://127.0.0.1:${port}/recon/pump`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ task_id: taskId, scenario })
  });
  if (!res.ok) {
    throw new Error(`pump(${scenario}, ${taskId}) failed: ${res.status} ${await res.text()}`);
  }
}

/** Call pump(), then wait for `waitFn` to confirm the events actually
 * landed; re-pump and retry if it doesn't, up to `retries` times. Even
 * with the earlier missing-id race fixed and per-state server isolation,
 * a one-shot pump occasionally still doesn't settle within a generous
 * polling window deep into a long batch — root cause not fully pinned
 * down (this harness's own tokio task scheduling under whatever load the
 * capture host is under at that moment, most likely). Re-firing the
 * scenario is safe (it only ever appends more of the same fixed-content
 * log lines) and makes the harness self-healing against whatever the
 * exact cause is, rather than chasing one more timeout number. */
async function pumpUntil(port, taskId, scenario, waitFn, retries = 3) {
  let lastErr;
  for (let i = 0; i < retries; i++) {
    await pump(port, taskId, scenario);
    try {
      await waitFn();
      return;
    } catch (e) {
      lastErr = e;
      console.log(`  pump(${scenario}) attempt ${i + 1}/${retries} did not settle: ${e.message}`);
    }
  }
  throw lastErr;
}

async function blockWebfonts(page) {
  await page.route(/fonts\.(googleapis|gstatic)\.com/, (route) => route.abort());
}
async function freezeClock(page) {
  await page.clock.install({ time: new Date(FROZEN_CLOCK) });
}

/** Track every task id created via a real POST /api/tasks response, in order. */
function trackCreatedIds(page) {
  const ids = [];
  page.on('response', async (r) => {
    if (r.url().includes('/api/tasks') && r.request().method() === 'POST') {
      const body = await r.json().catch(() => null);
      if (body && body.id) ids.push(body.id);
    }
  });
  return ids;
}

/** Block until `ids` (populated asynchronously by the `response` listener
 * above) has at least `count` entries. A fixed sleep here was a race: the
 * POST /api/tasks round trip does not reliably finish within any single
 * fixed delay, and calling /recon/pump with an undefined task_id fails
 * Json<PumpRequest> deserialization silently from this script's point of
 * view (capture.mjs never checked the pump response status) — the
 * fixture-server never even runs the scenario, so the state settles with 0
 * tokens and no live-output panel at all. Explicitly waiting for the id
 * removes the race outright, instead of hoping the timing window is wide
 * enough on every run. */
async function waitForIds(ids, count, timeoutMs = 8000) {
  const start = Date.now();
  while (ids.length < count && Date.now() - start < timeoutMs) {
    await sleep(50);
  }
  if (ids.length < count) {
    throw new Error(`expected ${count} created task id(s), only observed ${ids.length} after ${timeoutMs}ms`);
  }
}

async function addPane(page) {
  await page.evaluate(() => window.dispatchEvent(new Event('lopi:add-pane')));
  await sleep(100);
}

/** The pane-level "new prompt" composer's contenteditable — distinct from
 * the per-pane "stack command..." box at the bottom, which is also a
 * `.chipinput` and would otherwise collide with `.nth(paneIndex)` indexing. */
function goalInput(page, paneIndex) {
  return page.locator('.chipinput[data-placeholder="describe the prompt or goal..."]').nth(paneIndex);
}

/** Add one card to pane `paneIndex` (0-based) with `goal`, then run that pane's stack. */
// `force: true` skips Playwright's actionability "stable for two samples"
// wait — some ambient CSS motion (Census D territory) keeps these
// composer/dock elements' bounding boxes shifting by sub-pixel amounts
// indefinitely, so the natural-stability wait alone was timing out at
// 4-pane layouts. Elements are already on-screen at every viewport this
// script uses, so no explicit scroll is needed.
async function addCard(page, paneIndex, goal) {
  const ci = goalInput(page, paneIndex);
  await ci.click({ force: true, timeout: 15000 });
  await page.keyboard.type(goal);
  await sleep(150);
  const addBtn = page.getByRole('button', { name: 'add', exact: true }).nth(paneIndex);
  await addBtn.click({ force: true, timeout: 15000 });
  await sleep(150);
}

async function runPane(page, paneIndex) {
  const runBtn = page.getByRole('button', { name: 'run stack' }).nth(paneIndex);
  await runBtn.click({ force: true, timeout: 15000 });
  await sleep(200);
}

/** Once a pane starts running, its "run stack" button relabels to "pause" —
 * the collection of matching buttons shrinks by one on every click. Index-
 * based `.nth(i)` therefore drifts after the first run; always taking
 * `.first()` in a loop is the correct, order-independent way to run every
 * pane once. Same applies to `expandFirstOutput` below ("expand" becomes
 * "collapse" once clicked). */
async function runFirstPane(page) {
  await page.getByRole('button', { name: 'run stack' }).first().click({ force: true, timeout: 15000 });
  await sleep(200);
}

async function expandFirstOutput(page) {
  const btn = page.locator('button[title="expand"]').first();
  if (await btn.count()) {
    await btn.click({ force: true, timeout: 15000 });
  }
}

/** Add one card to pane `paneIndex` (0-based) with `goal`, then run that
 * pane's stack. Kept for the single-pane states — multi-pane states (S3,
 * S12) add every card first, then run every pane, so a growing "running"
 * card's layout in an earlier pane never destabilizes a later pane's
 * still-being-clicked composer. */
async function addAndRunCard(page, paneIndex, goal) {
  await addCard(page, paneIndex, goal);
  await runPane(page, paneIndex);
}

async function expandOutput(page, paneIndex) {
  const btn = page.locator('button[title="expand"]').nth(paneIndex);
  if (await btn.count()) {
    await btn.click({ force: true, timeout: 15000 });
  }
}

/** Poll for real evidence that a scenario's pumped events actually reached
 * the DOM, instead of trusting a fixed sleep. A fixed sleep here is not
 * just a race (like the missing-id bug above) — this repo's own CI-adjacent
 * sandbox showed the *same* one-shot scenario settle correctly every time
 * in isolation, but go dark once run back-to-back after S1-S4 inside one
 * long batch, purely from accumulated system load slowing down the tokio
 * task that fires the pump. A fixed delay picked against a quiet system is
 * not long enough under load; polling for the actual signal (the
 * live-output panel existing) is correct regardless of how slow the
 * system gets. */
async function waitForExpandButtons(page, count, timeoutMs = 15000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const n = await page.locator('button[title="expand"]').count();
    if (n >= count) return;
    await sleep(100);
  }
  throw new Error(`expected ${count} expand button(s) (live-output panel), only saw ${await page.locator('button[title="expand"]').count()} after ${timeoutMs}ms`);
}

/** Poll for `text` to appear anywhere on the page — used to wait for a
 * scenario's terminal marker instead of guessing how long it takes under
 * whatever load the system is under at capture time. */
async function waitForText(page, text, timeoutMs = 20000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const count = await page.getByText(text).count();
    if (count > 0) return;
    await sleep(150);
  }
  throw new Error(`expected text "${text}" to appear, timed out after ${timeoutMs}ms`);
}

/** Poll until every "RUNNING" badge is gone — the signal a pumped
 * `task_completed` (success/failure) actually reached the card, instead of
 * a fixed sleep that can undershoot under system load. */
async function waitForNoRunningBadges(page, timeoutMs = 15000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const n = await page.getByText('RUNNING', { exact: true }).count();
    if (n === 0) return;
    await sleep(150);
  }
  throw new Error(`expected all RUNNING badges to clear, timed out after ${timeoutMs}ms`);
}

// Fixed state -> port map (not index-based) so a single-state process
// invocation (see --state below) doesn't need to know the full state order
// to pick the same port a full-batch run would have used for that state.
const STATE_PORTS = { S1: 4150, S2: 4151, S3: 4152, S4: 4153, S5: 4154, S9: 4155, S10: 4156, S11: 4157, S12: 4158 };

// Reassigned per state in run() below — declared with `let` so the BUILDERS
// closures, which read PORT at call time rather than at closure-creation
// time, pick up whichever port is current.
let PORT = 4150;

/** One builder per reachable state: creates real card(s), pumps a
 * deterministic event scenario, expands the live-output panel, and returns
 * how long (ms) to wait before capturing the "settled" still. All delays
 * below are fixed constants — no randomness anywhere in this file. */
const BUILDERS = {
  async S1() {
    return { settleMs: 300 };
  },
  async S2(page) {
    const ids = trackCreatedIds(page);
    await addAndRunCard(page, 0, 'Add retry backoff to the webhook dispatcher');
    await waitForIds(ids, 1);
    await pumpUntil(PORT, ids[0], 'implementing', () => waitForExpandButtons(page, 1, 8000));
    await sleep(1500); // let a couple of the infinite loop's cycles land, for a fuller transcript
    await expandOutput(page, 0);
    return { settleMs: 800 };
  },
  async S3(page) {
    const ids = trackCreatedIds(page);
    await addPane(page);
    await addPane(page);
    const goals = [
      'Add retry backoff to the webhook dispatcher',
      'Wire OTel trace export from /generate spans',
      'Refactor the encoder hot path to use NEON unroll',
      'Migrate schedule chains to the v2 cron format'
    ];
    for (let i = 0; i < 4; i++) {
      await addCard(page, i, goals[i]);
    }
    for (let i = 0; i < 4; i++) {
      await runFirstPane(page);
    }
    await waitForIds(ids, 4);
    let lastErr;
    for (let attempt = 0; attempt < 3; attempt++) {
      for (let i = 0; i < 4; i++) {
        await pump(PORT, ids[i], 'implementing');
        await sleep(150);
      }
      try {
        await waitForExpandButtons(page, 4, 8000);
        lastErr = null;
        break;
      } catch (e) {
        lastErr = e;
        console.log(`  S3 pump attempt ${attempt + 1}/3 did not settle: ${e.message}`);
      }
    }
    if (lastErr) throw lastErr;
    await sleep(1500);
    for (let i = 0; i < 4; i++) await expandFirstOutput(page);
    return { settleMs: 800 };
  },
  async S4(page) {
    const ids = trackCreatedIds(page);
    await addAndRunCard(page, 0, 'Add a Redis-backed semantic cache to the retrieval path');
    await waitForIds(ids, 1);
    await pumpUntil(PORT, ids[0], 'streaming', () => waitForExpandButtons(page, 1, 8000));
    await sleep(1000);
    await expandOutput(page, 0);
    return { settleMs: 500, streamingTaskId: ids[0] };
  },
  async S5(page) {
    const ids = trackCreatedIds(page);
    await addAndRunCard(page, 0, 'Refactor scorer thresholds for the Konjo Verifier');
    await waitForIds(ids, 1);
    // This specific goal reproducibly causes a second, real POST /api/tasks
    // ~700-900ms after the first (server-side response carries
    // duplicate_of: <first-id> — confirmed via direct inspection, not
    // conjecture). Some client-side resubmission is genuinely firing twice;
    // which of the two ids ends up bound to the visible card isn't something
    // this read-only harness can control. Wait long enough to catch the
    // straggler, then pump every id seen so far — pumping a stale/orphaned
    // id is inert (broadcasts to a task_id nobody's subscribed to), so this
    // is safe regardless of which id the UI actually renders.
    await sleep(1500);
    let lastErr;
    for (const id of ids) {
      try {
        await pumpUntil(PORT, id, 'gate-failure', () => waitForText(page, 'Retrying · attempt', 8000));
        lastErr = null;
        break;
      } catch (e) {
        lastErr = e;
      }
    }
    if (lastErr) throw lastErr;
    await expandOutput(page, 0);
    return { settleMs: 500 };
  },
  async S9(page) {
    const ids = trackCreatedIds(page);
    await addAndRunCard(page, 0, 'Backfill task_logs pruning for the S9 recon load test');
    await waitForIds(ids, 1);
    await pumpUntil(PORT, ids[0], 'scrollback', () => waitForExpandButtons(page, 1, 8000));
    await expandOutput(page, 0);
    await waitForText(page, '[done] 2200 lines emitted for S9 scrollback census', 25000);
    return { settleMs: 500 };
  },
  async S10(page) {
    const ids = trackCreatedIds(page);
    await addAndRunCard(page, 0, 'Ingest a malformed vendor log export');
    await waitForIds(ids, 1);
    await pumpUntil(PORT, ids[0], 'pathological', () => waitForText(page, 'Recovered after 2 malformed records', 8000));
    await expandOutput(page, 0);
    return { settleMs: 500 };
  },
  async S11(page) {
    const ids = trackCreatedIds(page);
    // Two cards in ONE stack: card 2 depends on card 1 finishing (lopi's own
    // sequential pipeline model) — the closest real, honest match for "a
    // blocked task waiting on a dependency". See recon/REPORT.md section 7.
    const ci = goalInput(page, 0);
    await ci.click();
    await page.keyboard.type('Land the v2 cron schema migration');
    await sleep(150);
    await page.getByRole('button', { name: 'add', exact: true }).nth(0).click();
    await sleep(150);
    await ci.click();
    await page.keyboard.type('Backfill existing schedules onto the v2 cron schema');
    await sleep(150);
    await page.getByRole('button', { name: 'add', exact: true }).nth(0).click();
    await sleep(150);
    await page.getByRole('button', { name: 'run stack' }).nth(0).click();
    await waitForIds(ids, 1);
    await pumpUntil(PORT, ids[0], 'success', () => waitForNoRunningBadges(page, 8000));
    return { settleMs: 500 };
  },
  async S12(page) {
    const ids = trackCreatedIds(page);
    await addPane(page);
    await addPane(page);
    const goals = [
      'Add retry backoff to the webhook dispatcher',
      'Wire OTel trace export from /generate spans',
      'Document the budget degradation ladder',
      'Add a Redis-backed semantic cache to the retrieval path'
    ];
    for (let i = 0; i < 4; i++) {
      await addCard(page, i, goals[i]);
    }
    for (let i = 0; i < 4; i++) {
      await runFirstPane(page);
    }
    await waitForIds(ids, 4);
    let lastErr;
    for (let attempt = 0; attempt < 3; attempt++) {
      for (let i = 0; i < 4; i++) {
        await pump(PORT, ids[i], 'success');
        await sleep(150);
      }
      try {
        await waitForNoRunningBadges(page, 8000);
        lastErr = null;
        break;
      } catch (e) {
        lastErr = e;
        console.log(`  S12 pump attempt ${attempt + 1}/3 did not settle: ${e.message}`);
      }
    }
    if (lastErr) throw lastErr;
    return { settleMs: 500 };
  }
};

async function newPage(browser, viewport) {
  const context = await browser.newContext({ viewport: { width: viewport.width, height: viewport.height }, deviceScaleFactor: DPR });
  const page = await context.newPage();
  await blockWebfonts(page);
  await freezeClock(page);
  return { context, page };
}

async function gotoStacks(page, port) {
  await page.goto(`http://127.0.0.1:${port}/stacks`, { waitUntil: 'networkidle', timeout: 20000 });
  await page.addStyleTag({ content: FIXED_FONT_CSS });
}

async function extractColorCensus(page) {
  return page.evaluate(() => {
    function toHex(c) {
      const m = c.match(/rgba?\(([\d.]+),\s*([\d.]+),\s*([\d.]+)(?:,\s*([\d.]+))?\)/);
      if (!m) return null;
      const [r, g, b, a] = [Number(m[1]), Number(m[2]), Number(m[3]), m[4] === undefined ? 1 : Number(m[4])];
      if (a === 0) return null;
      return '#' + [r, g, b].map((n) => Math.round(n).toString(16).padStart(2, '0')).join('');
    }
    function luminance(hex) {
      const n = parseInt(hex.slice(1), 16);
      const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => {
        const s = v / 255;
        return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
      });
      return 0.2126 * r + 0.7152 * g + 0.0722 * b;
    }
    function contrast(a, b) {
      const la = luminance(a), lb = luminance(b);
      const [hi, lo] = la > lb ? [la, lb] : [lb, la];
      return (hi + 0.05) / (lo + 0.05);
    }
    const colorCounts = new Map();
    const pairs = [];
    for (const el of document.body.querySelectorAll('*')) {
      const rect = el.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) continue;
      const cs = getComputedStyle(el);
      const fg = toHex(cs.color);
      const bg = toHex(cs.backgroundColor);
      if (fg) colorCounts.set(fg, (colorCounts.get(fg) || 0) + 1);
      if (bg) colorCounts.set(bg, (colorCounts.get(bg) || 0) + 1);
      const hasOwnText = Array.from(el.childNodes).some((n) => n.nodeType === 3 && n.textContent.trim().length > 0);
      if (hasOwnText && fg && bg) {
        const cls = el.className && typeof el.className === 'string' ? '.' + el.className.split(' ').filter(Boolean).slice(0, 2).join('.') : '';
        pairs.push({
          selector: el.tagName.toLowerCase() + cls,
          fg, bg,
          fontSize: parseFloat(cs.fontSize),
          fontWeight: cs.fontWeight,
          contrast: Number(contrast(fg, bg).toFixed(2))
        });
      }
    }
    return { colors: Array.from(colorCounts.entries()).map(([hex, count]) => ({ hex, count })), textPairs: pairs };
  });
}

async function extractAnimationCensus(page) {
  return page.evaluate(() => {
    const running = document.getAnimations().map((a) => {
      const effect = a.effect;
      const timing = effect && effect.getTiming ? effect.getTiming() : {};
      let target = 'unknown';
      try {
        const t = effect.target;
        target = t ? t.tagName.toLowerCase() + (t.className && typeof t.className === 'string' ? '.' + t.className.split(' ').filter(Boolean).slice(0, 2).join('.') : '') : 'unknown';
      } catch { /* ignore */ }
      return { target, animationName: a.animationName || null, playState: a.playState, duration: timing.duration, iterations: timing.iterations };
    });
    const keyframes = [];
    for (const sheet of document.styleSheets) {
      let rules;
      try { rules = sheet.cssRules; } catch { continue; }
      for (const rule of rules) {
        if (rule.type === CSSRule.KEYFRAMES_RULE) keyframes.push({ name: rule.name, frameCount: rule.cssRules.length });
      }
    }
    const seen = new Set();
    const transitions = [];
    for (const el of document.body.querySelectorAll('*')) {
      const cs = getComputedStyle(el);
      if (cs.transitionDuration && cs.transitionDuration !== '0s') {
        const key = `${cs.transitionProperty}|${cs.transitionDuration}`;
        if (!seen.has(key)) { seen.add(key); transitions.push({ property: cs.transitionProperty, duration: cs.transitionDuration, easing: cs.transitionTimingFunction }); }
      }
    }
    return { running, keyframes, transitions, reducedMotionRespected: matchMedia('(prefers-reduced-motion: reduce)').matches };
  });
}

async function measureCLS(page, windowMs) {
  await page.evaluate(() => {
    window.__clsEntries = [];
    window.__clsObserver = new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        if (!entry.hadRecentInput) {
          window.__clsEntries.push({
            value: entry.value,
            time: entry.startTime,
            sources: (entry.sources || []).map((s) => ({
              node: s.node ? s.node.tagName + (s.node.className && typeof s.node.className === 'string' ? '.' + s.node.className.split(' ').filter(Boolean).slice(0, 2).join('.') : '') : null,
              previousRect: s.previousRect, currentRect: s.currentRect
            }))
          });
        }
      }
    });
    window.__clsObserver.observe({ type: 'layout-shift', buffered: true });
  });
  await sleep(windowMs);
  return page.evaluate(() => {
    window.__clsObserver.disconnect();
    const entries = window.__clsEntries || [];
    const total = entries.reduce((s, e) => s + e.value, 0);
    const worst = [...entries].sort((a, b) => b.value - a.value).slice(0, 5);
    return { total, worst, count: entries.length };
  });
}

async function measureMutations(page, windowMs) {
  await page.evaluate(() => {
    window.__mutations = [];
    window.__mutObserver = new MutationObserver((records) => {
      const now = performance.now();
      for (const r of records) {
        let chars = 0;
        for (const n of r.addedNodes) chars += (n.textContent || '').length;
        if (r.type === 'characterData') chars += (r.target.textContent || '').length;
        window.__mutations.push({ t: now, type: r.type, addedNodes: r.addedNodes.length, chars });
      }
    });
    window.__mutObserver.observe(document.body, { childList: true, subtree: true, characterData: true, characterDataOldValue: true });
  });
  await sleep(windowMs);
  return page.evaluate((windowMs) => {
    window.__mutObserver.disconnect();
    const muts = window.__mutations || [];
    const sizes = muts.map((m) => m.chars).filter((c) => c > 0).sort((a, b) => a - b);
    const pct = (p) => (sizes.length ? sizes[Math.min(sizes.length - 1, Math.floor((p / 100) * sizes.length))] : 0);
    return {
      totalMutations: muts.length,
      windowMs,
      mutationsPerSecond: Number((muts.length / (windowMs / 1000)).toFixed(2)),
      medianChars: pct(50),
      p95Chars: pct(95),
      maxChars: sizes.length ? sizes[sizes.length - 1] : 0
    };
  }, windowMs);
}

// --state <name> restricts a single invocation to one state, run to
// completion in its own node process. Unique-port-per-state (fixing the
// S5 zombie-server false-positive health check) still left one flake
// (a "0 task ids observed" race at S3, never before seen in ~5 prior
// full-batch runs that always failed at S5 specifically) — pointing at
// cumulative resource contention across the whole long-running node/V8
// process rather than anything port- or server-specific. Running each
// state as a separate process, orchestrated by run-states.sh, gives every
// state a fully fresh OS process, heap, and event loop, with a real
// process-exit boundary between states instead of just a browser/server
// teardown inside one continuously running process.
const stateArgIdx = process.argv.indexOf('--state');
const requestedState = stateArgIdx !== -1 ? process.argv[stateArgIdx + 1] : null;

async function run() {
  await mkdir(SHOTS_DIR, { recursive: true });
  await mkdir(RAW_DIR, { recursive: true });

  const states = requestedState ? [requestedState] : Object.keys(BUILDERS);
  if (requestedState && !BUILDERS[requestedState]) {
    throw new Error(`unknown --state ${requestedState}`);
  }

  for (const state of states) {
    console.log(`\n=== ${state} ===`);
    const raw = { state, viewports: {} };

    // Fresh fixture-server and fresh browser per state, on a port unique to
    // this state (STATE_PORTS above) so a not-yet-torn-down previous
    // server's /api/health can never be mistaken for this state's server.
    PORT = STATE_PORTS[state];
    const browser = await chromium.launch({ executablePath: CHROME_PATH });
    const child = spawnFixture(PORT);
    try {
      await waitForHealth(PORT);

      for (const viewport of VIEWPORTS) {
        console.log(`  viewport ${viewport.name}`);
        const { context, page } = await newPage(browser, viewport);
        await gotoStacks(page, PORT);

        const built = await BUILDERS[state](page);

        // Still (motion=off).
        await page.addStyleTag({ content: MOTION_OFF_CSS });
        await sleep(built.settleMs + 250);
        const stillFile = path.join(SHOTS_DIR, `${state}_${viewport.name}_off.png`);
        await page.screenshot({ path: stillFile });

        const colorCensus = await extractColorCensus(page);
        const animationCensusOff = await extractAnimationCensus(page);

        // Frame strip (motion=on) — re-navigate fresh so the motion-off
        // override never applies, then rebuild the same state.
        const { context: context2, page: page2 } = await newPage(browser, viewport);
        await gotoStacks(page2, PORT);
        const built2 = await BUILDERS[state](page2);
        await sleep(built2.settleMs);
        const frames = [];
        for (let i = 1; i <= 20; i++) {
          const f = path.join(SHOTS_DIR, `${state}_${viewport.name}_on_frame${String(i).padStart(2, '0')}.png`);
          await page2.screenshot({ path: f });
          frames.push(f);
          await sleep(100);
        }
        const animationCensusOn = await extractAnimationCensus(page2);

        let cls;
        let mutations;
        if (['S4', 'S9', 'S10'].includes(state) && viewport.name === '1440x900') {
          cls = await measureCLS(page2, 4000);
        }
        if (state === 'S4' && viewport.name === '1440x900') {
          mutations = await measureMutations(page2, 4000);
        }

        raw.viewports[viewport.name] = { still: stillFile, frames, colorCensus, animationCensusOff, animationCensusOn };
        if (cls) raw.cls = cls;
        if (mutations) raw.mutations = mutations;

        await context.close();
        await context2.close();
      }

      if (state === 'S4') {
        console.log('  recording 30s video...');
        const context = await browser.newContext({
          viewport: { width: 1440, height: 900 },
          deviceScaleFactor: DPR,
          recordVideo: { dir: SHOTS_DIR, size: { width: 1440, height: 900 } }
        });
        const page = await context.newPage();
        await blockWebfonts(page);
        await freezeClock(page);
        await gotoStacks(page, PORT);
        await BUILDERS.S4(page);
        await sleep(30000);
        const video = page.video();
        await context.close();
        const savedPath = await video.path();
        const finalPath = path.join(SHOTS_DIR, 'S4_streaming_30s.webm');
        await rename(savedPath, finalPath);
        raw.videoPath = finalPath;
      }

      await writeFile(path.join(RAW_DIR, `${state}.json`), JSON.stringify(raw, null, 2));
    } finally {
      await browser.close();
      child.kill('SIGTERM');
      await sleep(300); // let the OS release the port before the next state's spawn
    }
  }
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
