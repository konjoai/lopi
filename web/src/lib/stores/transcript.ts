/**
 * Transcript model — turns the flat `AgentEvent` stream into the ordered list of
 * rich blocks the chat pane renders (assistant markdown, thinking, tool-call
 * accordions, status chips).
 *
 * Why a dedicated layer: `agents.ts` reduces events into *current* per-agent
 * state (the orb inputs); the Pulse feed (`events.ts`) keeps a global ring of
 * one-line summaries. Neither preserves the per-session, in-order narrative a
 * transcript needs, so this module owns that — as pure functions (testable in
 * isolation) plus a thin Svelte store updated from the same `applyMessage` path.
 *
 * Wire→block mapping (see `crates/lopi-agent/src/claude_events.rs`): assistant
 * text and thinking arrive as `log_line`s (plain, or `💭`-prefixed); tool calls
 * arrive *both* as a `🔧` log line and a structured `tool_call` event, so the
 * `🔧` log lines are dropped here and the structured event is the source of
 * truth. `tool_result` pairs back to its `tool_call`; everything else degrades
 * to a status chip and nothing ever throws.
 */
import { writable, type Readable } from 'svelte/store';
import type { AgentEvent } from '$lib/types';

/** Severity tier for a status chip — drives its color. */
export type ChipTier = 'info' | 'good' | 'warn' | 'bad';

/** A single rendered transcript block. */
export type TranscriptBlock =
  | { kind: 'assistant_text'; id: string; text: string; streaming: boolean }
  | { kind: 'thinking'; id: string; text: string }
  | {
      kind: 'tool_call';
      id: string;
      tool: string;
      args: string;
      result?: { preview: string; isError: boolean };
    }
  | { kind: 'status'; id: string; tier: ChipTier; label: string };

/** Max blocks retained per session — bounds memory on very long runs. */
export const MAX_BLOCKS = 600;

// ── Pure helpers ──────────────────────────────────────────────────────────────

/** The trailing block iff it is an assistant_text block still open for writing. */
function openText(blocks: TranscriptBlock[]): Extract<TranscriptBlock, { kind: 'assistant_text' }> | null {
  const last = blocks[blocks.length - 1];
  return last && last.kind === 'assistant_text' && last.streaming ? last : null;
}

/** Close any open assistant_text block so the streaming caret stops. */
function sealOpenText(blocks: TranscriptBlock[]): TranscriptBlock[] {
  const open = openText(blocks);
  if (!open) return blocks;
  const next = blocks.slice();
  next[next.length - 1] = { ...open, streaming: false };
  return next;
}

/** Append (or extend) an assistant_text block with one more line. */
function appendText(blocks: TranscriptBlock[], line: string, id: string): TranscriptBlock[] {
  const open = openText(blocks);
  if (open) {
    const next = blocks.slice();
    next[next.length - 1] = { ...open, text: `${open.text}\n${line}` };
    return next;
  }
  return [...blocks, { kind: 'assistant_text', id, text: line, streaming: true }];
}

/** Append (or extend) a thinking block with one more line. */
function appendThinking(blocks: TranscriptBlock[], line: string, id: string): TranscriptBlock[] {
  const sealed = sealOpenText(blocks);
  const last = sealed[sealed.length - 1];
  if (last && last.kind === 'thinking') {
    const next = sealed.slice();
    next[next.length - 1] = { ...last, text: `${last.text}\n${line}` };
    return next;
  }
  return [...sealed, { kind: 'thinking', id, text: line }];
}

/** Push a status chip, sealing any open text first. */
function pushStatus(
  blocks: TranscriptBlock[],
  tier: ChipTier,
  label: string,
  id: string
): TranscriptBlock[] {
  return [...sealOpenText(blocks), { kind: 'status', id, tier, label }];
}

/**
 * Tier for a synthetic status line whose severity isn't in its glyph alone —
 * inferred from the same verdict/decision keywords the Rust side formats the
 * line with (`crates/lopi-agent/src/runner/{eval_runner,verifier_runner,
 * progress}.rs`), so e.g. `verdict=error` (a judge/infra failure) reads
 * distinctly from `verdict=fail` (a real content rejection) instead of both
 * collapsing into one generic "info" pill — or, before this, into
 * indistinguishable plain assistant text (see module doc: these all arrive
 * as `log_line`s, and an unrecognized prefix used to fall through to
 * `appendText`, rendering literal `verdict=error` dumps as if Claude had
 * said them).
 */
function inferredTier(t: string): ChipTier {
  if (/verdict=error|errored|fail-closed/.test(t)) return 'bad';
  if (/verdict=pass|passed=true|gain gate: gain\b/.test(t)) return 'good';
  if (/rejected|verdict=fail|passed=false|regression|failed/.test(t)) return 'warn';
  return 'info';
}

/** Route a `log_line` to the right block kind by its glyph prefix. */
function reduceLogLine(blocks: TranscriptBlock[], line: string, level: string, id: string): TranscriptBlock[] {
  const t = line.trim();
  if (!t) return blocks;
  if (t.startsWith('🔧')) return blocks; // structured tool_call is the source of truth
  if (t.startsWith('💭')) return appendThinking(blocks, t.replace(/^💭\s*/, ''), id);
  if (t.startsWith('●')) return pushStatus(blocks, 'info', t.replace(/^●\s*/, ''), id);
  if (t.startsWith('⛔')) return pushStatus(blocks, 'bad', t.replace(/^⛔\s*/, ''), id);
  // Eval-tier (🎯), verifier (🔬), gain gate (📈), schema validation (📐) —
  // all synthetic status, not Claude output; without this they fell through
  // to plain assistant markdown, indistinguishable from something Claude
  // actually said (the bug this closes).
  if (/^[🎯🔬📈📐]/.test(t)) {
    const glyph = t.slice(0, t.indexOf(' ') === -1 ? t.length : t.indexOf(' '));
    return pushStatus(blocks, inferredTier(t), t.replace(new RegExp(`^${glyph}\\s*`), ''), id);
  }
  if (level === 'error') return pushStatus(blocks, 'bad', t, id);
  return appendText(blocks, t, id);
}

/** Attach a tool_result to its originating tool_call, or stand it up alone. */
function reduceToolResult(
  blocks: TranscriptBlock[],
  tool: string,
  preview: string,
  isError: boolean,
  id: string
): TranscriptBlock[] {
  const sealed = sealOpenText(blocks);
  for (let i = sealed.length - 1; i >= 0; i--) {
    const b = sealed[i];
    if (b.kind === 'tool_call' && b.tool === tool && !b.result) {
      const next = sealed.slice();
      next[i] = { ...b, result: { preview, isError } };
      return next;
    }
  }
  return [...sealed, { kind: 'tool_call', id, tool, args: '', result: { preview, isError } }];
}

function statusLabel(status: unknown): string {
  if (typeof status === 'string') return status;
  if (status && typeof status === 'object') return Object.keys(status)[0] ?? 'Unknown';
  return 'Unknown';
}

/**
 * Fold one event into a session's block list. Pure: returns a new array (or the
 * same reference when the event contributes nothing). `id` seeds any new block's
 * stable render key and must be unique per call within a session.
 */
export function appendEvent(blocks: TranscriptBlock[], ev: AgentEvent, id: string): TranscriptBlock[] {
  const out = appendEventInner(blocks, ev, id);
  return out.length > MAX_BLOCKS ? out.slice(out.length - MAX_BLOCKS) : out;
}

function appendEventInner(blocks: TranscriptBlock[], ev: AgentEvent, id: string): TranscriptBlock[] {
  switch (ev.type) {
    case 'log_line':
      return reduceLogLine(blocks, ev.line, ev.level, id);
    case 'tool_call':
      return [...sealOpenText(blocks), { kind: 'tool_call', id, tool: ev.tool, args: ev.summary }];
    case 'tool_result':
      return reduceToolResult(blocks, ev.tool, ev.preview, ev.is_error, id);
    case 'plan_proposed':
      return [...sealOpenText(blocks), { kind: 'assistant_text', id, text: ev.plan, streaming: false }];
    case 'phase':
      return pushStatus(blocks, 'info', `phase · ${ev.phase}`, id);
    case 'status_changed':
      return pushStatus(blocks, 'info', `→ ${statusLabel(ev.status)} · attempt ${ev.attempt}`, id);
    case 'score_updated':
      return pushStatus(
        blocks,
        ev.test_pass_rate >= 0.8 ? 'good' : 'warn',
        `scored ${Math.round(ev.test_pass_rate * 100)}% · ${ev.lint_errors} lint · ${ev.diff_lines} Δ`,
        id
      );
    case 'verifier_verdict':
      return pushStatus(
        blocks,
        ev.passed ? 'good' : 'warn',
        ev.passed ? 'verifier: all criteria met' : `verifier: ${ev.gaps.length} gap(s)`,
        id
      );
    case 'api_retry':
      return pushStatus(blocks, 'warn', `rate limit · ${ev.limit_type} ${Math.round(ev.utilization * 100)}%`, id);
    case 'cost':
      return pushStatus(blocks, 'info', `$${ev.cost_usd.toFixed(4)} · ${ev.num_turns} turns`, id);
    case 'task_completed': {
      const failed = typeof ev.outcome === 'object' && 'Failed' in ev.outcome;
      return pushStatus(
        blocks,
        failed ? 'bad' : 'good',
        failed ? 'failed — retries exhausted' : `completed in ${ev.total_attempts} attempt(s)`,
        id
      );
    }
    case 'task_cancelled':
      return pushStatus(blocks, 'warn', 'cancelled', id);
    default:
      return blocks;
  }
}

// ── Store ─────────────────────────────────────────────────────────────────────

const store = writable<Map<string, TranscriptBlock[]>>(new Map());

/** Read-only view of every session's transcript blocks. */
export const transcripts: Readable<Map<string, TranscriptBlock[]>> = store;

let seq = 0;

/** Owning task id for an event, or null for fleet-scoped frames. */
function eventTaskId(ev: AgentEvent): string | null {
  if (ev.type === 'pool_stats') return null;
  return (ev as { task_id?: string | null }).task_id ?? null;
}

/** Fold one live event into its session transcript. Called from `applyMessage`. */
export function recordTranscript(ev: AgentEvent): void {
  const taskId = eventTaskId(ev);
  if (!taskId) return;
  seq += 1;
  store.update((m) => {
    const cur = m.get(taskId) ?? [];
    const next = appendEvent(cur, ev, `b${seq}`);
    if (next === cur) return m;
    const out = new Map(m);
    out.set(taskId, next);
    return out;
  });
}

/** Drop a session's transcript (called when a session is permanently deleted). */
export function clearTranscript(taskId: string): void {
  store.update((m) => {
    if (!m.has(taskId)) return m;
    const out = new Map(m);
    out.delete(taskId);
    return out;
  });
}
