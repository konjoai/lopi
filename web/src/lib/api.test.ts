/**
 * API client tests — runs as a standalone Node script with a mocked
 * global fetch. Usage: `npx tsx src/lib/api.test.ts` from web/
 */
import {
  ApiError,
  listTasks,
  recentLogs,
  createSchedule,
  enableSchedule,
  deleteDlq,
  getConfig,
  getLoopEngineering,
  setLoopStrategy,
  setLoopEscalation
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
  mockFetch(200, { tasks: [{ id: 'a', goal: 'g', status: 'Queued', created_at: '', completed_at: null }] });
  const t = await listTasks();
  eq(t.tasks.length, 1, 'listTasks returns parsed body');
  eq(captured[0].path, '/api/tasks', 'listTasks hits /api/tasks');

  // Query params are encoded.
  mockFetch(200, { logs: [] });
  await recentLogs(42);
  eq(captured[0].path, '/api/logs?n=42', 'recentLogs passes limit');

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
  await deleteDlq('d1');
  eq(captured[0].init?.method, 'DELETE', 'deleteDlq DELETEs');

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
