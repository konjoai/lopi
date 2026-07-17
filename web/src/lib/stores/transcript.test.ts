/**
 * Pure transcript-reducer tests — run with `npx tsx src/lib/stores/transcript.test.ts`.
 * No browser, no Svelte: just `(blocks, AgentEvent) → blocks` folds. Verifies the
 * wire→block mapping, tool_call/tool_result pairing, text/thinking merging, the
 * streaming-caret seal, and the per-session block cap.
 */
import { appendEvent, MAX_BLOCKS, type TranscriptBlock } from './transcript';
import type { AgentEvent } from '$lib/types';
import { eq, ok, namedSummary } from '$lib/test-harness';

const ev = (e: Record<string, unknown>) => e as unknown as AgentEvent;
const ID = 'task-1';
const log = (line: string, level = 'info') => ev({ type: 'log_line', task_id: ID, line, level, ts: '' });

let n = 0;
const fold = (blocks: TranscriptBlock[], e: AgentEvent) => appendEvent(blocks, e, `b${n++}`);

// ── plain log lines accumulate into one open assistant_text block ─────────────
{
  let b: TranscriptBlock[] = [];
  b = fold(b, log('Hello'));
  b = fold(b, log('world'));
  eq(b.length, 1, 'two plain lines → one block');
  ok(b[0].kind === 'assistant_text', 'block is assistant_text');
  eq((b[0] as { text: string }).text, 'Hello\nworld', 'lines joined with newline');
  ok((b[0] as { streaming: boolean }).streaming, 'open block streams (caret on)');
}

// ── a tool_call seals the open text block and the result pairs back ───────────
{
  let b: TranscriptBlock[] = [];
  b = fold(b, log('thinking about it'));
  b = fold(b, ev({ type: 'tool_call', task_id: ID, tool: 'Bash', summary: 'ls -la' }));
  ok(!(b[0] as { streaming: boolean }).streaming, 'tool_call seals prior text (caret off)');
  eq(b.length, 2, 'text + tool_call');
  ok(b[1].kind === 'tool_call', 'second block is tool_call');
  b = fold(b, ev({ type: 'tool_result', task_id: ID, tool: 'Bash', is_error: false, preview: 'a\nb' }));
  eq(b.length, 2, 'tool_result nests, no new block');
  const tc = b[1] as Extract<TranscriptBlock, { kind: 'tool_call' }>;
  eq(tc.result, { preview: 'a\nb', isError: false }, 'result attached to its tool_call');
}

// ── the 🔧 log line is dropped (structured tool_call is the source of truth) ──
{
  let b: TranscriptBlock[] = [];
  b = fold(b, log('🔧 Bash(ls -la)'));
  eq(b.length, 0, '🔧 log line contributes no block');
}

// ── 💭 lines become a thinking block, merged across lines ─────────────────────
{
  let b: TranscriptBlock[] = [];
  b = fold(b, log('💭 first'));
  b = fold(b, log('💭 second'));
  eq(b.length, 1, 'two 💭 lines → one thinking block');
  ok(b[0].kind === 'thinking', 'block is thinking');
  eq((b[0] as { text: string }).text, 'first\nsecond', 'thinking text merged, glyph stripped');
}

// ── an orphan tool_result stands alone rather than crashing ───────────────────
{
  let b: TranscriptBlock[] = [];
  b = fold(b, ev({ type: 'tool_result', task_id: ID, tool: 'Read', is_error: true, preview: 'boom' }));
  eq(b.length, 1, 'orphan result → standalone block');
  const tc = b[0] as Extract<TranscriptBlock, { kind: 'tool_call' }>;
  ok(tc.kind === 'tool_call' && tc.result?.isError === true, 'standalone carries the error result');
}

// ── status-bearing events degrade to chips ────────────────────────────────────
{
  let b: TranscriptBlock[] = [];
  b = fold(b, ev({ type: 'phase', task_id: ID, phase: 'review_ready' }));
  b = fold(b, ev({ type: 'cost', task_id: ID, cost_usd: 0.1234, num_turns: 3, session_id: 's' }));
  b = fold(b, ev({ type: 'task_completed', task_id: ID, outcome: { Failed: { reason: 'x' } }, total_attempts: 2 }));
  eq(b.length, 3, 'three chips');
  ok(b.every((x) => x.kind === 'status'), 'all are status chips');
  eq((b[2] as { tier: string }).tier, 'bad', 'failed completion is a bad chip');
}

// ── error log line becomes a bad chip, not appended prose ─────────────────────
{
  let b: TranscriptBlock[] = [];
  b = fold(b, log('something broke', 'error'));
  ok(b[0].kind === 'status' && (b[0] as { tier: string }).tier === 'bad', 'error line → bad chip');
}

// ── unknown / no-op events never throw and never add blocks ────────────────────
{
  let b: TranscriptBlock[] = [{ kind: 'status', id: 'x', tier: 'info', label: 'seed' }];
  const before = b;
  b = fold(b, ev({ type: 'turn_metrics', task_id: ID, pressure: 0.5, activity: 0.5, tokens_per_sec: 1, cost_usd: 0 }));
  ok(b === before, 'turn_metrics is a no-op (same reference)');
}

// ── 🎯/🔬/📈/📐 synthetic status lines become chips, not plain prose ──────────
{
  // Before this fix these fell through to `appendText` — an eval verdict
  // dump rendering indistinguishably from real Claude output.
  let b: TranscriptBlock[] = [];
  b = fold(b, log('🎯 eval: verdict=error score=0.500 (2 check(s))'));
  ok(b[0].kind === 'status', 'eval line becomes a status chip, not prose');
  eq((b[0] as { tier: string }).tier, 'bad', 'verdict=error is a bad chip (infra/config, not content)');
  eq((b[0] as { label: string }).label, 'eval: verdict=error score=0.500 (2 check(s))', 'glyph stripped from label');
}
{
  let b: TranscriptBlock[] = [];
  b = fold(b, log('🎯 eval: verdict=pass score=1.000 (2 check(s))'));
  eq((b[0] as { tier: string }).tier, 'good', 'verdict=pass is a good chip');
}
{
  let b: TranscriptBlock[] = [];
  b = fold(b, log('🎯 eval rejected — 2 critique item(s); appending for next attempt'));
  eq((b[0] as { tier: string }).tier, 'warn', 'a rejected eval is a warn chip');
}
{
  let b: TranscriptBlock[] = [];
  b = fold(b, log('🔬 verifier: passed=true confidence=90% gaps=0'));
  eq((b[0] as { tier: string }).tier, 'good', 'verifier passed=true is a good chip');
}
{
  let b: TranscriptBlock[] = [];
  b = fold(b, log('🔬 verifier errored — fail-closed: blocking finalize and retrying'));
  eq((b[0] as { tier: string }).tier, 'bad', 'verifier erroring is a bad chip');
}
{
  let b: TranscriptBlock[] = [];
  b = fold(b, log('📈 gain gate: gain (weighted=0.850)'));
  eq((b[0] as { tier: string }).tier, 'good', 'a gain-gate gain is a good chip');
}
{
  let b: TranscriptBlock[] = [];
  b = fold(b, log('📐 output_schema validation failed (1 issue(s)):'));
  eq((b[0] as { tier: string }).tier, 'warn', 'schema validation failure is a warn chip');
}

// ── block list is capped at MAX_BLOCKS ────────────────────────────────────────
{
  let b: TranscriptBlock[] = [];
  for (let i = 0; i < MAX_BLOCKS + 50; i++) {
    b = fold(b, ev({ type: 'phase', task_id: ID, phase: `p${i}` }));
  }
  eq(b.length, MAX_BLOCKS, 'block list capped at MAX_BLOCKS');
  ok((b[b.length - 1] as { label: string }).label.includes(`p${MAX_BLOCKS + 49}`), 'newest block kept');
}

namedSummary('transcript');
