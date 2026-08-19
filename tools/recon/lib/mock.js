// Dev-only network mocking for Sprint U-0 recon — reaches every dashboard
// state deterministically via page.route() REST stubs and a scripted
// WebSocket replay (page.routeWebSocket), instead of a live lopi backend.
// Precedent: web/e2e/popover-visibility.spec.ts already mocks the backend
// this way for the existing Playwright suite.
'use strict';

/** Baseline REST fixtures shared by every state — a healthy, non-empty
 *  offline-banner-free shell. Individual states override fields as needed. */
function baseRestFixtures() {
  return {
    '**/api/repos': {
      repos: [
        { owner: 'konjoai', name: 'lopi', path: '~/lopi' },
        { owner: 'konjoai', name: 'kyro', path: '~/kyro' }
      ]
    },
    '**/api/branches*': { branches: ['main', 'feat/recon-sprint'], default: 'main' },
    '**/api/claude-commands*': { commands: [] },
    '**/api/models': {
      models: [
        { id: 'claude-opus-5', display_name: 'Opus 5', effort: ['low', 'medium', 'high'] },
        { id: 'claude-sonnet-5', display_name: 'Sonnet 5', effort: ['low', 'medium', 'high'] },
        { id: 'claude-haiku-4-5', display_name: 'Haiku 4.5', effort: ['low', 'medium'] }
      ]
    },
    '**/api/stats': {
      running: 1,
      queued: 0,
      succeeded: 12,
      failed: 1,
      uptime_secs: 3600,
      total_tokens_today: 482000,
      total_cost_usd_today: 4.12,
      synthetic: true,
      measurement_provenance: { kind: 'measured', source: 'recon fixture' }
    },
    '**/api/economics': {
      active: true,
      tier: 'full',
      headroom_usd: 42.5,
      pool_kind: 'shared',
      pool_ceiling_usd: 50.0,
      cost_per_merged_pr_usd: 0.87,
      cost_per_gate_pass_usd: 0.31,
      cost_per_retry_usd: 0.14,
      cache_attributed_saving_usd: 2.03,
      pool_runway_days: 6.5
    },
    '**/api/quota': {
      five_hour: { status: 'ok', utilization: 0.22, resets_at: '2026-07-30T17:00:00Z', observed_at: '2026-07-30T12:00:00Z' },
      seven_day: { status: 'ok', utilization: 0.41, resets_at: '2026-08-06T12:00:00Z', observed_at: '2026-07-30T12:00:00Z' }
    },
    '**/api/config': { repo_path: '~/lopi', max_concurrent: 4 },
    '**/api/version': { version: '0.37.0' }
  };
}

/** Install page.route() handlers for the REST fixtures, with per-state
 *  overrides shallow-merged over the baseline. */
async function installRestMocks(page, overrides = {}) {
  const fixtures = { ...baseRestFixtures(), ...overrides };
  for (const [pattern, body] of Object.entries(fixtures)) {
    await page.route(pattern, (route) =>
      route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) })
    );
  }
  // Anything else under /api/** that isn't explicitly fixtured: empty 200
  // rather than a 500, so an unmocked endpoint degrades quietly instead of
  // flipping the whole page into the OFFLINE banner.
  await page.route('**/api/**', (route) => {
    route.fulfill({ status: 200, contentType: 'application/json', body: '{}' });
  });
}

/**
 * Intercept the app's single WebSocket ('/ws'). Returns a promise resolving
 * to the WebSocketRoute handle once the page's client connects, so a caller
 * can `send()` scripted WireMessage frames (see web/src/lib/types.ts) on its
 * own schedule — deliberately AFTER any composer/run-stack UI scripting
 * (`seedCard`) has bound a card to a real (mocked) task id, rather than on a
 * fire-and-forget timer from connect time. See fixtures/states.js.
 */
/**
 * Returns `{ armed, connected }`:
 *  - `armed` resolves once the route is REGISTERED — the caller must
 *    `await` this before `page.goto()`. An unawaited `routeWebSocket()`
 *    races navigation and silently loses (the real WS request fails before
 *    the route arms).
 *  - `connected` resolves to the `WebSocketRoute` once the page's client
 *    actually opens the socket (after navigation) — await this later, right
 *    before sending scripted frames.
 */
function installWsReplay(page) {
  let resolveConnected;
  const connected = new Promise((resolve) => {
    resolveConnected = resolve;
  });
  const armed = page.routeWebSocket('**/ws', (ws) => {
    resolveConnected(ws);
  });
  return { armed, connected };
}

/** Send a scripted sequence of {atMs, message} frames on an open
 *  WebSocketRoute, waiting the real `atMs` delta between each via `page`'s
 *  clock (so motion capture timing — S4's frame strip and video — is
 *  faithful to the fixture's intended pacing). */
async function sendSequence(page, wsRoute, script) {
  let last = 0;
  for (const { atMs, message } of script) {
    const delta = Math.max(0, atMs - last);
    if (delta > 0) await page.waitForTimeout(delta);
    last = atMs;
    wsRoute.send(JSON.stringify(message));
  }
}

/**
 * Mock `POST /api/tasks` to return a fixed id looked up by the request's
 * `goal` field, and every other method/endpoint under /api/tasks with an
 * empty 200 — so a state's scripted WS events (keyed to the same fixed id)
 * bind to the exact card that requested it. `goalToTaskId` is a plain
 * object of `{ [goal]: taskId }`.
 */
async function installTaskCreateMock(page, goalToTaskId) {
  await page.route('**/api/tasks', async (route) => {
    if (route.request().method() !== 'POST') {
      return route.fulfill({ status: 200, contentType: 'application/json', body: '{}' });
    }
    const body = route.request().postDataJSON();
    const id = goalToTaskId[body.goal] || `task-unmapped-${Object.keys(goalToTaskId).length}`;
    await route.fulfill({
      status: 200,
      contentType: 'application/json',
      body: JSON.stringify({ id, queued: true, duplicate_of: null, client_ref: body.client_ref ?? null })
    });
  });
}

module.exports = { baseRestFixtures, installRestMocks, installWsReplay, sendSequence, installTaskCreateMock };
