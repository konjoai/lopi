/**
 * Wire types — mirror lopi-core Rust enums verbatim.
 *
 * Source: crates/lopi-core/src/{task.rs, event.rs}
 *
 * Serde representation contracts:
 *  - `AgentEvent` uses `#[serde(tag = "type", rename_all = "snake_case")]`
 *    → tagged unions on `type` field with snake_case values.
 *  - `TaskStatus` uses default serde (no rename, no tag)
 *    → unit variants are bare strings ("Queued"); tuple/struct variants are
 *      single-key objects ({"Retrying": {"attempt": 2}}).
 *  - `LogLevel` uses `#[serde(rename_all = "lowercase")]`.
 *  - `Priority` is `Low`/`Normal`/`High`/`Critical` (PascalCase, default repr).
 *
 * If any of these change in lopi-core, this file MUST be updated. The parser
 * (parser.ts) provides runtime validation as a defence-in-depth check.
 */

// ── Primitives ────────────────────────────────────────────────────────────────
export type Priority = 'Low' | 'Normal' | 'High' | 'Critical';
export type LogLevel = 'info' | 'warn' | 'error' | 'debug';

// ── TaskStatus (default serde representation) ─────────────────────────────────
export type TaskStatus =
  | 'Queued'
  | 'Planning'
  | 'Implementing'
  | 'Testing'
  | 'Scoring'
  | 'RolledBack'
  | { Retrying: { attempt: number } }
  | { Success: { branch: string; pr_url: string | null } }
  | { Failed: { reason: string } };

// ── AgentEvent (#[serde(tag = "type", rename_all = "snake_case")]) ────────────
export type AgentEvent =
  | { type: 'task_queued'; task_id: string; goal: string; priority: Priority }
  | { type: 'task_started'; task_id: string; attempt: number; branch: string; repo?: string }
  | {
      type: 'status_changed';
      task_id: string;
      status: TaskStatus;
      attempt: number;
    }
  | {
      type: 'log_line';
      task_id: string;
      line: string;
      level: LogLevel;
      ts: string; // ISO-8601
    }
  | {
      type: 'score_updated';
      task_id: string;
      test_pass_rate: number;
      lint_errors: number;
      diff_lines: number;
    }
  | {
      type: 'task_completed';
      task_id: string;
      outcome: TaskStatus;
      total_attempts: number;
    }
  | { type: 'task_cancelled'; task_id: string }
  | {
      type: 'pool_stats';
      running: number;
      queued: number;
      succeeded: number;
      failed: number;
      uptime_secs: number;
    }
  // Added in UI-2 / lopi-core v0.9.0 — emitted periodically by AgentRunner.
  // Drives the Forge's live shader uniforms directly.
  | {
      type: 'turn_metrics';
      task_id: string;
      pressure: number; // 0..1, ContextWindow.token_pressure()
      activity: number; // 0..1, normalized tokens/sec
      tokens_per_sec: number;
      cost_usd: number;
    }
  // Adversarial verifier outcome — emitted after the scoring phase.
  | {
      type: 'verifier_verdict';
      task_id: string;
      passed: boolean;
      gaps: string[];
      fix_hints: string[];
    }
  // Budget guard refused further spend in a rolling 1h window.
  | {
      type: 'budget_exceeded';
      task_id: string | null;
      scope: BudgetScope;
      limit_usd: number;
      burned_usd: number;
    };

/** Which budget scope refused (mirrors lopi-core `BudgetScope`). */
export type BudgetScope = 'fleet' | 'agent' | 'task';

// ── Snapshot sent on WebSocket connect ────────────────────────────────────────
export interface SnapshotTask {
  id: string;
  goal: string;
  status: TaskStatus | string;
  created_at: string;
}

export interface PoolStats {
  running: number;
  queued: number;
  succeeded: number;
  failed: number;
  uptime_secs: number;
}

export interface SnapshotMessage {
  type: 'snapshot';
  tasks: SnapshotTask[];
  stats: PoolStats;
}

export type WireMessage = AgentEvent | SnapshotMessage;

// ── Phase (UI concept — derived from TaskStatus) ──────────────────────────────
// The Forge visualizes this. There are six phases mapped from TaskStatus:
//
//   TaskStatus           → Phase
//   ────────────────────────────────────
//   Queued               → Boot
//   Planning             → Planning
//   Implementing         → Implementation
//   Testing              → Testing
//   Scoring              → Conclusion
//   Retrying { ... }     → Discovery
//   Success { ... }      → Conclusion
//   Failed { ... }       → Conclusion
//   RolledBack           → Conclusion
export type Phase =
  | 'Boot'
  | 'Discovery'
  | 'Planning'
  | 'Implementation'
  | 'Testing'
  | 'Conclusion';
