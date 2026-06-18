/**
 * Agent UI model — the shape the Forge renders, plus the small pure helpers
 * that build and normalise it. No Svelte, no browser: imported by both the
 * store (`agents.ts`) and the pure reducer (`agents-reducer.ts`), and unit
 * tested directly.
 */
import type { Phase, TaskStatus } from '$lib/types';
import type { StimulusKind } from '$lib/forge/excitement';

/** Coarse lifecycle status used by the UI (distinct from the wire `TaskStatus`). */
export type Status = 'queued' | 'running' | 'completed' | 'failed' | 'cancelled';

/** Everything a pane needs to render one agent, derived from the event stream. */
export interface AgentState {
  id: string;
  goal: string;
  repo: string;
  branch: string;
  status: Status;
  taskStatus: TaskStatus | string;
  phase: Phase;
  attempt: number;
  startedAt: number;
  elapsedMs: number;

  // Forge inputs (0..1 each)
  pressure: number;
  activity: number;
  health: number;

  // Score breakdown (from score_updated events)
  testPassRate?: number;
  lintErrors?: number;
  diffLines?: number;
  score?: number; // synthetic 0..1 composite

  // Adversarial verifier verdict (from verifier_verdict events)
  verifierPassed?: boolean;
  verifierGaps?: string[];
  verifierFixHints?: string[];

  cost: number; // USD accumulated
  thought?: string; // last log line (preview)

  /**
   * Timestamp (ms) of the last incoming request/stimulus for this agent —
   * drives the Forge orb's react animation (shake → fast spin → orange glow).
   */
  stimulus: number;
  /**
   * What excited the orb last: 'request' (ember orange), 'success'
   * (jade bloom) or 'failure' (rose flare).
   */
  stimulusKind: StimulusKind;
}

/** A single log line scoped to a task. */
export interface LogEntry {
  ts: number;
  taskId: string;
  level: 'info' | 'warn' | 'error' | 'debug';
  message: string;
}

/** Phase color map (mirrors the `:root` vars in app.css). */
export const PHASE_COLORS: Record<Phase, string> = {
  Boot: '#f5f5f5',
  Discovery: '#00d4ff',
  Planning: '#00ffd4',
  Implementation: '#ff4500',
  Testing: '#ffcc00',
  Conclusion: '#00ff9d'
};

/** A fresh, neutral agent — the base the reducer spreads onto. */
export function makeBlank(id: string): AgentState {
  return {
    id,
    goal: 'unknown',
    repo: '',
    branch: '',
    status: 'queued',
    taskStatus: 'Queued',
    phase: 'Boot',
    attempt: 0,
    startedAt: Date.now(),
    elapsedMs: 0,
    pressure: 0.05,
    activity: 0,
    health: 0.85,
    cost: 0,
    stimulus: 0,
    stimulusKind: 'request'
  };
}

/** Clamp to [0, 1]; non-finite input collapses to 0. */
export function clamp01(n: number): number {
  if (!Number.isFinite(n)) return 0;
  if (n < 0) return 0;
  if (n > 1) return 1;
  return n;
}
