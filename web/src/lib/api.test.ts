/**
 * API client tests — runs as a standalone Node script with a mocked
 * global fetch. Usage: `npx tsx src/lib/api.test.ts` from web/
 */
import {
  ApiError,
  getStats,
  createTask,
  createSchedule,
  enableSchedule,
  deleteSchedule,
  getConfig,
  getLoopEngineering,
  setLoopStrategy,
  setLoopEscalation,
  createMaxx,
  enableMaxx,
  getQuota
} from './api';
import { eqIs as eq, summary } from '$lib/test-harness';

interface Captured {
  path: string;
  init?: RequestInit;
}
let captured: Captured[] = [];

function mockFetch(status: number, body: unknown) {
  captured = [];
  (globalThis as { fetch: unknown }).fetch = (path: string, init?: RequestInit) => {
    captured.push({ path, init });
    return Promise.resolve({
      ok: status >= 200 && status < 300,
      status,
      json: () => Promise.resolve(body)
    });
  };
}

function mockNetworkFailure() {
  (globalThis as { fetch: unknown }).fetch = () => Promise.reject(new Error('boom'));
}

async function main() {
  // Happy path: response body passes through.
  mockFetch(200, {
    running: 1, queued: 0, succeeded: 3, failed: 0, uptime_secs: 10,
    total_tokens_today: 800, total_cost_usd_today: 0.048
  });
  const stats = await getStats();
  eq(stats.total_cost_usd_today, 0.048, 'getStats returns parsed body');
  eq(captured[0].path, '/api/stats', 'getStats hits /api/stats');

  // POST bodies are JSON with content-type.
  mockFetch(200, {});
  await createSchedule({ name: 'n', cron: '* * * * * *', goal: 'g' });
  eq(captured[0].init?.method, 'POST', 'createSchedule POSTs');
  eq(
    (captured[0].init?.headers as Record<string, string>)['content-type'],
    'application/json',
    'createSchedule sends JSON'
  );
  eq(
    JSON.parse(String(captured[0].init?.body)).name,
    'n',
    'createSchedule serializes body'
  );

  // Backend-1: an empty repo is omitted from the body entirely, not sent
  // as `""` — the server's `Option<String>` falls back to its own
  // configured repo only when the key is absent; a present empty string
  // deserializes to `Some("")`, which fails outright trying to open a git
  // repo at an empty path.
  mockFetch(200, { id: 't', goal: 'g', queued: true, duplicate_of: null, client_ref: null });
  await createTask('g', '', 'normal');
  eq('repo' in JSON.parse(String(captured[0].init?.body)), false, 'createTask omits repo when blank');

  mockFetch(200, { id: 't', goal: 'g', queued: true, duplicate_of: null, client_ref: null });
  await createTask('g', '/some/repo', 'normal');
  eq(JSON.parse(String(captured[0].init?.body)).repo, '/some/repo', 'createTask sends repo when set');

  // Path segments are URI-encoded.
  mockFetch(200, { id: 'x', enabled: true });
  await enableSchedule('has space');
  eq(
    captured[0].path,
    '/api/schedules/has%20space/enable',
    'enableSchedule encodes id'
  );

  // DELETE method wiring.
  mockFetch(200, { deleted: 'd1' });
  await deleteSchedule('d1');
  eq(captured[0].init?.method, 'DELETE', 'deleteSchedule DELETEs');

  // MAXX: create sends the favorability fields through untouched.
  mockFetch(200, { id: 'm1', name: 'n', headroom_gate: true, windows: ['five_hour'] });
  await createMaxx({
    name: 'overnight',
    goal: 'work the backlog',
    quiet_hours: [23, 7],
    headroom_gate: true,
    windows: ['five_hour', 'seven_day']
  });
  eq(captured[0].path, '/api/maxx', 'createMaxx hits /api/maxx');
  eq(captured[0].init?.method, 'POST', 'createMaxx POSTs');
  eq(
    JSON.stringify(JSON.parse(String(captured[0].init?.body)).quiet_hours),
    JSON.stringify([23, 7]),
    'createMaxx serializes quiet_hours'
  );

  // MAXX: path segments are URI-encoded, same as schedules.
  mockFetch(200, { id: 'x', enabled: true });
  await enableMaxx('has space');
  eq(captured[0].path, '/api/maxx/has%20space/enable', 'enableMaxx encodes id');

  // Quota: GET with no body, response passes through untouched (including
  // a null window for "never observed").
  mockFetch(200, {
    five_hour: { status: 'allowed', utilization: 0.1, resets_at: 123, observed_at: 't' },
    seven_day: null
  });
  const quota = await getQuota();
  eq(captured[0].path, '/api/quota', 'getQuota hits /api/quota');
  eq(quota.five_hour?.utilization, 0.1, 'getQuota returns five_hour snapshot');
  eq(quota.seven_day, null, 'getQuota preserves null for an unobserved window');

  // Loop Engineering: snapshot read carries the self-prompt catalog.
  mockFetch(200, {
    repo: '/r',
    config: { self_prompt: 'direct', self_prompt_tag: 'S1' },
    self_prompt_strategies: [{ value: 'reflexion', tag: 'S2', label: 'Reflexion', preview: 'p' }]
  });
  const loop = await getLoopEngineering();
  eq(captured[0].path, '/api/loop-engineering', 'getLoopEngineering hits endpoint');
  eq(loop.self_prompt_strategies[0].value, 'reflexion', 'snapshot carries strategies');

  // Loop Engineering: strategy write POSTs the chosen tag.
  mockFetch(200, { self_prompt: 'reflexion', self_prompt_tag: 'S2', self_prompt_label: 'Reflexion' });
  const saved = await setLoopStrategy('reflexion');
  eq(captured[0].path, '/api/loop-engineering/strategy', 'setLoopStrategy hits endpoint');
  eq(captured[0].init?.method, 'POST', 'setLoopStrategy POSTs');
  eq(JSON.parse(String(captured[0].init?.body)).strategy, 'reflexion', 'sends strategy tag');
  eq(saved.self_prompt_tag, 'S2', 'returns saved strategy');

  // Loop Engineering: escalation toggle POSTs the enabled flag.
  mockFetch(200, { escalate_strategy: true });
  const esc = await setLoopEscalation(true);
  eq(captured[0].path, '/api/loop-engineering/escalation', 'setLoopEscalation hits endpoint');
  eq(JSON.parse(String(captured[0].init?.body)).enabled, true, 'sends enabled flag');
  eq(esc.escalate_strategy, true, 'returns escalation state');

  // Error body message surfaces in ApiError.
  mockFetch(404, { error: 'schedule not found' });
  try {
    await getConfig();
    eq(true, false, 'non-2xx should throw');
  } catch (e) {
    eq(e instanceof ApiError, true, 'throws ApiError on 404');
    eq((e as ApiError).status, 404, 'ApiError carries status');
    eq((e as ApiError).message, 'schedule not found', 'ApiError carries server message');
  }

  // Non-JSON error body falls back to HTTP status text.
  mockFetch(500, undefined);
  try {
    await getConfig();
    eq(true, false, 'non-2xx should throw');
  } catch (e) {
    eq((e as ApiError).message, 'HTTP 500', 'falls back to HTTP status');
  }

  // Network failure maps to status 0.
  mockNetworkFailure();
  try {
    await getConfig();
    eq(true, false, 'network failure should throw');
  } catch (e) {
    eq((e as ApiError).status, 0, 'network failure → status 0');
    eq((e as ApiError).message, 'backend unreachable', 'network failure message');
  }

  summary();
}

main();
