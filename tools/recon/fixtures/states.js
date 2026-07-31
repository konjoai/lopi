// Sprint U-0 fixture definitions — S1-S13, deterministic content.
//
// Determinism contract: fixed task ids ("task-sNN"), fixed goal/log text
// (no Math.random/crypto.randomUUID anywhere in this file), and every
// WireMessage timestamp is a fixed ISO string. The only non-fixed input is
// wall-clock delay between scripted WS frames (`atMs`), which governs
// animation/motion capture timing, not content — recorded per capture run in
// LEDGER.md as "fixture seed: static (no PRNG)".
'use strict';

const TASK = (n) => `task-s${n}`;

/** A single log_line WireMessage. */
function log(taskId, line, level = 'info', atMs = 0) {
  return { atMs, message: { type: 'log_line', task_id: taskId, line, level, ts: '2026-07-30T12:00:00Z' } };
}

function statusChanged(taskId, status, attempt = 1, atMs = 0) {
  return { atMs, message: { type: 'status_changed', task_id: taskId, status, attempt } };
}

function taskStarted(taskId, branch, repo, atMs = 0) {
  return { atMs, message: { type: 'task_started', task_id: taskId, attempt: 1, branch, repo } };
}

function taskCompleted(taskId, outcome, total_attempts = 1, atMs = 0) {
  return { atMs, message: { type: 'task_completed', task_id: taskId, outcome, total_attempts } };
}

function turnMetrics(taskId, pressure, activity, atMs = 0) {
  return {
    atMs,
    message: { type: 'turn_metrics', task_id: taskId, pressure, activity, tokens_per_sec: activity * 80, cost_usd: 0.02 }
  };
}

/** 2,000+ deterministic scrollback lines for S9 — same template, indexed. */
function longScrollback(taskId, n = 2200) {
  const out = [];
  for (let i = 0; i < n; i++) {
    out.push(log(taskId, `[${String(i).padStart(4, '0')}] cargo check: crate lopi-agent, file ${i % 37}.rs — 0 warnings`, 'debug', 0));
  }
  return out;
}

/** One 4,000-char unbroken string (no spaces) plus one line carrying raw ANSI
 *  SGR escapes — S10's "pathological content" pair. */
function pathologicalContent(taskId) {
  const unbroken = 'x'.repeat(4000);
  const ansiLine = '[31mERROR[0m: [1mbuild failed[0m at [4msrc/lib.rs:42[0m';
  return [log(taskId, unbroken, 'info', 0), log(taskId, ansiLine, 'error', 0)];
}

/**
 * State table. `rest` shallow-overrides the REST fixture baseline
 * (lib/mock.js baseRestFixtures). `ws` is the scripted WireMessage replay.
 * `seedCard(page)` performs any composer/UI scripting needed to get a card
 * bound to the fixture's task id(s) before the WS script fires — required
 * because stack panes are 100% client-side (no server "stack" concept, see
 * web/src/lib/stores/stackRun.ts's module doc).
 */
const STATES = {
  S1_cold_start_empty: {
    label: 'S1 — cold start, empty',
    rest: { '**/api/stats': { running: 0, queued: 0, succeeded: 0, failed: 0, uptime_secs: 4, total_tokens_today: 0, total_cost_usd_today: 0, synthetic: true } },
    ws: [],
    seedCard: null
  },

  S2_one_agent_running: {
    label: 'S2 — one agent running, mid-stage',
    rest: {},
    ws: [
      taskStarted(TASK(2), 'lopi/task-s2-attempt-1', '~/lopi'),
      statusChanged(TASK(2), 'Implementing'),
      log(TASK(2), 'Read 6 files, 900 lines analyzed', 'info'),
      log(TASK(2), 'Editing crates/lopi-agent/src/runner.rs', 'info'),
      turnMetrics(TASK(2), 0.4, 0.6)
    ],
    seedCard: { taskId: TASK(2), goal: 'fixture: one agent running mid-stage' }
  },

  S3_four_agents_running: {
    label: 'S3 — four agents running concurrently',
    rest: { '**/api/stats': { running: 4, queued: 0, succeeded: 8, failed: 0, uptime_secs: 900, total_tokens_today: 210000, total_cost_usd_today: 1.9, synthetic: true } },
    ws: [3, 4, 5, 6].flatMap((n) => [
      taskStarted(TASK(n), `lopi/task-s${n}-attempt-1`, '~/lopi'),
      statusChanged(TASK(n), ['Planning', 'Implementing', 'Testing', 'Scoring'][n - 3]),
      log(TASK(n), `stage ${['Planning', 'Implementing', 'Testing', 'Scoring'][n - 3]} in progress`, 'info')
    ]),
    seedCard: [3, 4, 5, 6].map((n) => ({ taskId: TASK(n), goal: `fixture: concurrent agent ${n - 2}` }))
  },

  S4_streaming_now: {
    label: 'S4 — an agent streaming output right now',
    rest: {},
    ws: [
      taskStarted(TASK(4), 'lopi/task-s4-attempt-1', '~/lopi', 0),
      statusChanged(TASK(4), 'Implementing', 1, 0),
      // Staggered log lines over ~4s to produce real motion for the
      // motion=on frame strip and the 30s video (Step 3).
      log(TASK(4), "I'll add the caching layer. Plan:", 'info', 200),
      log(TASK(4), '1. Wrap the retrieval path in a cache check', 'info', 900),
      log(TASK(4), '2. TTL-based eviction', 'info', 1600),
      log(TASK(4), 'Editing crates/lopi-agent/src/retrieve.rs', 'info', 2300),
      turnMetrics(TASK(4), 0.35, 0.7, 500),
      turnMetrics(TASK(4), 0.5, 0.62, 1500),
      turnMetrics(TASK(4), 0.44, 0.8, 2500),
      log(TASK(4), 'cargo check: clean', 'info', 3200),
      log(TASK(4), 'cargo nextest: 12 passed, 0 failed', 'info', 3900)
    ],
    seedCard: { taskId: TASK(4), goal: 'fixture: streaming output live' }
  },

  S5_gate_failure: {
    label: 'S5 — gate failure with the failure record displayed',
    rest: {},
    ws: [
      taskStarted(TASK(5), 'lopi/task-s5-attempt-1', '~/lopi'),
      statusChanged(TASK(5), 'Scoring'),
      {
        atMs: 300,
        message: {
          type: 'verifier_verdict',
          task_id: TASK(5),
          passed: false,
          gaps: ['error path for empty input is untested', 'public fn missing rustdoc'],
          fix_hints: ['add a unit test covering the empty case']
        }
      },
      log(TASK(5), 'gate FAILED: clippy -D warnings — 1 violation', 'error', 500),
      taskCompleted(TASK(5), { Failed: { reason: 'gate: clippy -D warnings failed (1 violation)' } }, 1, 700)
    ],
    seedCard: { taskId: TASK(5), goal: 'fixture: gate failure record' }
  },

  S6_dead_letter: {
    label: 'S6 — task in dead-letter after exhausted attempts',
    rest: {},
    ws: [
      taskStarted(TASK(6), 'lopi/task-s6-attempt-3', '~/lopi'),
      statusChanged(TASK(6), { Retrying: { attempt: 3 } }),
      log(TASK(6), 'attempt 3/3: tests still red after fix', 'error', 200),
      taskCompleted(TASK(6), { Failed: { reason: 'exhausted max_retries (3) — see task logs' } }, 3, 400)
    ],
    seedCard: { taskId: TASK(6), goal: 'fixture: dead-letter after exhausted attempts' }
  },

  S7_budget_degraded: {
    label: 'S7 — budget tier degraded to Conserve, then Drain (Sprint E)',
    rest: { '**/api/economics': { active: true, tier: 'conserve', headroom_usd: 3.2, pool_kind: 'shared', pool_ceiling_usd: 50.0, cost_per_merged_pr_usd: 0.9, cost_per_gate_pass_usd: 0.3, cost_per_retry_usd: 0.2, cache_attributed_saving_usd: 1.1, pool_runway_days: 0.4 } },
    ws: [],
    seedCard: null,
    unreachable: 'No component under web/src reads /api/economics (grep confirmed zero references). The tier ladder has no web surface to photograph — see REPORT.md §9/§11.'
  },

  S8_cache_degraded: {
    label: 'S8 — cache hit ratio below threshold, degradation warning (Sprint C)',
    rest: {},
    ws: [],
    seedCard: null,
    unreachable: 'No cache-hit-ratio degradation banner/component found anywhere under web/src (grep for cache-hit/degrad concepts found no UI consumer). See REPORT.md §9/§11.'
  },

  S9_long_scrollback: {
    label: 'S9 — long scrollback, 2,000+ log lines',
    rest: {},
    ws: [
      taskStarted(TASK(9), 'lopi/task-s9-attempt-1', '~/lopi'),
      statusChanged(TASK(9), 'Implementing'),
      ...longScrollback(TASK(9), 2200)
    ],
    seedCard: { taskId: TASK(9), goal: 'fixture: long scrollback 2000+ lines' }
  },

  S10_pathological_content: {
    label: 'S10 — pathological content: 4,000-char unbroken string; ANSI-escape line',
    rest: {},
    ws: [
      taskStarted(TASK(10), 'lopi/task-s10-attempt-1', '~/lopi'),
      statusChanged(TASK(10), 'Implementing'),
      ...pathologicalContent(TASK(10))
    ],
    seedCard: { taskId: TASK(10), goal: 'fixture: pathological content' }
  },

  S11_blocked_on_dependency: {
    label: 'S11 — blocked task waiting on a dependency',
    rest: {},
    ws: [
      taskStarted(TASK(11), 'lopi/task-s11-attempt-1', '~/lopi'),
      statusChanged(TASK(11), 'Implementing'),
      log(TASK(11), 'card 1 of 2 running; card 2 waiting in chain order', 'info')
    ],
    // Two cards in one pane: card 1 runs (bound to task-s11), card 2 stays
    // 'queued' client-side behind it — the client's actual model of "waiting
    // on a dependency" (see web/src/lib/stores/stack.ts's CardStatus doc:
    // its own 'blocked' means "a run that ended in error", NOT "waiting to
    // start" — a naming collision worth flagging, see REPORT.md §9).
    seedCard: [
      { taskId: TASK(11), goal: 'fixture: chain step 1 (running)' },
      { taskId: null, goal: 'fixture: chain step 2 (queued behind step 1)', leaveQueued: true }
    ]
  },

  S12_all_finished_green: {
    label: 'S12 — all finished, all green',
    rest: { '**/api/stats': { running: 0, queued: 0, succeeded: 16, failed: 0, uptime_secs: 7200, total_tokens_today: 610000, total_cost_usd_today: 5.4, synthetic: true } },
    ws: [12, 13, 14].flatMap((n, i) => [
      taskStarted(TASK(n), `lopi/task-s${n}-attempt-1`, '~/lopi'),
      taskCompleted(TASK(n), { Success: { branch: `lopi/task-s${n}-attempt-1`, pr_url: `https://github.com/konjoai/lopi/pull/${200 + i}` } })
    ]),
    seedCard: [12, 13, 14].map((n) => ({ taskId: TASK(n), goal: `fixture: finished green ${n}` }))
  },

  S13_loop_stacks_populated: {
    label: 'S13 — Loop Stacks page populated: several stacks, mixed statuses, aliases and chips',
    rest: {},
    ws: [
      taskStarted(TASK(13), 'lopi/task-s13-attempt-1', '~/lopi'),
      statusChanged(TASK(13), 'Implementing')
    ],
    // Composer density fixture: many chip tokens across two panes to
    // exercise wrapping/overflow, per the brief's own S13 description.
    seedCard: {
      taskId: TASK(13),
      goal: 'fixture: populated stacks :alpha @lopi ;model:opus ;effort:high ×3',
      extraCards: [
        'fixture: second card :bravo @kyro ;model:sonnet ;effort:medium ×2',
        'fixture: third card :charlie @lopi ;model:haiku ;effort:low'
      ]
    }
  }
};

module.exports = { STATES, TASK };
