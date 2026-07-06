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
    case 'tool_call': {
      const cur = next.get(ev.task_id) ?? makeBlank(ev.task_id);
      const label = ev.summary ? `🔧 ${ev.tool}(${ev.summary})` : `🔧 ${ev.tool}`;
      next.set(ev.task_id, {
        ...cur,
        lastTool: ev.tool,
        toolCalls: (cur.toolCalls ?? 0) + 1,
        thought: label,
        // A tool call is real activity — keep the orb lively.
        activity: clamp01(Math.max(cur.activity, 0.6)),
        stimulus: Date.now(),
        stimulusKind: 'request'
      });
      break;
    }
    case 'tool_result': {
      const cur = next.get(ev.task_id);
      if (!cur) break;
      next.set(ev.task_id, {
        ...cur,
        // An errored tool result nudges health down; a clean one is neutral.
        health: ev.is_error ? clamp01(cur.health * 0.9) : cur.health,
        stimulusKind: ev.is_error ? 'failure' : cur.stimulusKind
      });
      break;
    }
    case 'token_delta': {
      const cur = next.get(ev.task_id);
      if (!cur) break;
      // Output tokens accumulate across turns; input/cache are per-turn.
      const outputTokens = (cur.outputTokens ?? 0) + ev.output_tokens;
      next.set(ev.task_id, {
        ...cur,
        outputTokens,
        inputTokens: ev.input_tokens,
        cacheReadTokens: ev.cache_read_tokens,
        // Token flow is generation intensity — normalize against a soft cap.
        activity: clamp01(ev.output_tokens / 200)
      });
      break;
    }
    case 'api_retry': {
      const cur = next.get(ev.task_id);
      if (!cur) break;
      next.set(ev.task_id, {
        ...cur,
        throttled: ev.status !== 'allowed',
        utilization: clamp01(ev.utilization),
        thought: `⏳ rate limit ${ev.limit_type} ${Math.round(ev.utilization * 100)}%`,
        stimulus: Date.now(),
        stimulusKind: 'request'
      });
      break;
    }
    case 'cost': {
      const cur = next.get(ev.task_id);
      if (!cur) break;
      next.set(ev.task_id, {
        ...cur,
        cost: ev.cost_usd,
        numTurns: ev.num_turns,
        sessionId: ev.session_id || cur.sessionId
      });
      break;
    }
    case 'phase': {
      const cur = next.get(ev.task_id);
      if (!cur) break;
      next.set(ev.task_id, { ...cur, claudePhase: ev.phase });
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
