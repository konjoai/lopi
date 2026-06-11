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
  getConfig
} from './api';

let pass = 0;
let fail = 0;

function eq(actual: unknown, expected: unknown, name: string) {
  if (Object.is(actual, expected)) {
    pass++;
  } else {
    fail++;
    console.error(`✗ ${name}: expected ${expected}, got ${actual}`);
  }
}

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

  console.log(`\n── Result: ${pass} passed, ${fail} failed ──`);
  if (fail > 0) process.exit(1);
}

main();
