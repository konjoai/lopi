/**
 * Agent reducer — the single source of truth for `AgentEvent → AgentState`
 * mutation, split out of `agents.ts` to keep that module under the size gate.
 *
 * `reduce` is pure: it returns a new map and never touches the store, so it can
 * be reasoned about (and tested) in isolation. `makeBlank` seeds a placeholder
 * agent for events that arrive before their `task_queued`, and is reused by the
 * snapshot hydration path in `agents.ts`.
 */
import { taskStatusToPhase, isTerminalStatus } from '$lib/parser';
import type { AgentEvent } from '$lib/types';
import type { AgentState } from './agents';

/** Seed a placeholder agent — used for out-of-order events and snapshot rows. */
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

/** Apply one `AgentEvent` to the agent map, returning a new map. */
export function reduce(
  map: Map<string, AgentState>,
  ev: AgentEvent
): Map<string, AgentState> {
  const next = new Map(map);
  switch (ev.type) {
    case 'task_queued': {
      next.set(ev.task_id, {
        id: ev.task_id,
        goal: ev.goal,
        repo: '',
        branch: '',
        status: 'queued',
        taskStatus: 'Queued',
        phase: 'Boot',
        attempt: 0,
        startedAt: Date.now(),
        elapsedMs: 0,
        pressure: 0.05,
        activity: 0.0,
        health: 0.85,
        cost: 0,
        stimulus: Date.now(),
        stimulusKind: 'request'
      });
      break;
    }
    case 'task_started': {
      const cur = next.get(ev.task_id);
      next.set(ev.task_id, {
        ...(cur ?? makeBlank(ev.task_id)),
        status: 'running',
        attempt: ev.attempt,
        branch: ev.branch,
        repo: ev.repo ?? cur?.repo ?? '',
        startedAt: cur?.startedAt ?? Date.now(),
        phase: cur?.phase ?? 'Boot',
        stimulus: Date.now(),
        stimulusKind: 'request'
      });
      break;
    }
    case 'status_changed': {
      const cur = next.get(ev.task_id);
      if (!cur) break;
      const phase = taskStatusToPhase(ev.status);
      const isCompleted = isTerminalStatus(ev.status);
      const isAwaiting =
        typeof ev.status === 'object' && 'AwaitingPlanApproval' in ev.status;
      next.set(ev.task_id, {
        ...cur,
        taskStatus: ev.status,
        phase,
        attempt: ev.attempt,
        // Cleared once the agent advances past the gate (approved → implement).
        awaitingApproval: isAwaiting ? cur.awaitingApproval : false,
        status: isCompleted
          ? typeof ev.status === 'object' && 'Failed' in ev.status
            ? 'failed'
            : 'completed'
          : 'running'
      });
      break;
    }
    case 'log_line': {
      const cur = next.get(ev.task_id);
      if (cur) {
        // Keep the most recent meaningful line as the "thought" preview.
        next.set(ev.task_id, { ...cur, thought: ev.line });
      }
      break;
    }
    case 'score_updated': {
      const cur = next.get(ev.task_id);
      if (!cur) break;
      // Composite 0..1 score: primarily test_pass_rate, penalized by lint errors.
      const composite = clamp01(
        ev.test_pass_rate * 0.85 - Math.min(ev.lint_errors / 50, 0.15)
      );
      next.set(ev.task_id, {
        ...cur,
        testPassRate: ev.test_pass_rate,
        lintErrors: ev.lint_errors,
        diffLines: ev.diff_lines,
        score: composite,
        // Health drifts toward score over time — recent runs influence the aura.
        health: cur.health * 0.7 + composite * 0.3
      });
      break;
    }
    case 'task_completed': {
      const cur = next.get(ev.task_id);
      if (!cur) break;
      const failed = typeof ev.outcome === 'object' && 'Failed' in ev.outcome;
      next.set(ev.task_id, {
        ...cur,
        status: failed ? 'failed' : 'completed',
        taskStatus: ev.outcome,
        phase: 'Conclusion',
        activity: 0.0,
        attempt: ev.total_attempts,
        // Terminal flash: jade bloom on success, rose flare on failure.
        stimulus: Date.now(),
        stimulusKind: failed ? 'failure' : 'success'
      });
      break;
    }
    case 'task_cancelled': {
      const cur = next.get(ev.task_id);
      if (cur) {
        next.set(ev.task_id, { ...cur, status: 'cancelled', activity: 0, awaitingApproval: false });
      }
      break;
    }
    case 'plan_proposed': {
      const cur = next.get(ev.task_id);
      if (!cur) break;
      next.set(ev.task_id, {
        ...cur,
        awaitingApproval: true,
        planSteps: ev.steps,
        planText: ev.plan,
        // A gentle ember pulse marks the pause-for-review moment.
        stimulus: Date.now(),
        stimulusKind: 'request'
      });
      break;
    }
    case 'pool_stats': {
      // Handled separately via the `poolStats` store — no per-agent change.
      break;
    }
    case 'turn_metrics': {
      const cur = next.get(ev.task_id);
      if (!cur) break;
      next.set(ev.task_id, {
        ...cur,
        pressure: ev.pressure,
        activity: ev.activity,
        cost: ev.cost_usd
      });
      break;
    }
    case 'verifier_verdict': {
      const cur = next.get(ev.task_id);
      if (!cur) break;
      next.set(ev.task_id, {
        ...cur,
        verifierPassed: ev.passed,
        verifierGaps: ev.gaps,
        verifierFixHints: ev.fix_hints,
        // Pulse the orb in the verdict's color — jade pass, rose fail.
        stimulus: Date.now(),
        stimulusKind: ev.passed ? 'success' : 'failure'
      });
      break;
    }
    case 'budget_exceeded': {
      // Fleet-wide breach (null task_id) is handled by the events store as
      // a global alert; a task-scoped breach also flares that agent's orb.
      if (ev.task_id) {
        const cur = next.get(ev.task_id);
        if (cur) {
          next.set(ev.task_id, {
            ...cur,
            stimulus: Date.now(),
            stimulusKind: 'failure'
          });
        }
      }
      break;
    }
  }
  return next;
}

function clamp01(n: number): number {
  if (!Number.isFinite(n)) return 0;
  if (n < 0) return 0;
  if (n > 1) return 1;
  return n;
}
