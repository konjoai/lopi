/**
 * Cross-agent insight detection — pure logic, no Three.js.
 *
 * Two agents are connected when they share a real-world signal worth
 * surfacing. Each signal contributes to a strength score in [0, 1] which
 * the renderer converts to line opacity and pulse intensity.
 *
 * Signals (in priority order):
 *   1. Same repo (base: 0.55)
 *      Two agents editing the same codebase have a real risk of conflict
 *      and a real opportunity for shared learning. The most actionable
 *      insight in the constellation.
 *   2. Same phase right now (bonus: +0.25)
 *      When connected agents are simultaneously in the same lifecycle
 *      phase, the line glows brighter — visualizes synchrony.
 *   3. Goal keyword overlap (bonus: up to +0.20)
 *      Tokenized goals share ≥ 2 non-trivial words → real semantic link.
 *      Stop words ("the", "a", "to", etc.) excluded.
 *
 * The function is deliberately conservative: only emits connections with
 * strength > 0 AND involving distinct agents AND with a non-empty
 * shared signal. Cluttering the scene with weak associations would defeat
 * the purpose of the view.
 */
import type { AgentState } from '$lib/stores/agents';

export interface Connection {
  /** Canonical id: `{minId}|{maxId}` so each pair is unique regardless of order */
  id: string;
  fromId: string;
  toId: string;
  /** 0..1 — drives opacity and glow */
  strength: number;
  /** Human-readable reason — used in hover tooltip */
  reasons: string[];
  /** True when both agents are currently in the same Phase */
  phaseSync: boolean;
}

const STOP_WORDS = new Set([
  'the', 'a', 'an', 'and', 'or', 'but', 'to', 'of', 'in', 'on', 'at', 'for',
  'with', 'by', 'as', 'is', 'are', 'was', 'were', 'be', 'been', 'being',
  'this', 'that', 'these', 'those', 'add', 'fix', 'use', 'new', 'all'
]);

function tokenize(text: string): Set<string> {
  return new Set(
    text
      .toLowerCase()
      .split(/[^a-z0-9_-]+/)
      .filter((t) => t.length >= 3 && !STOP_WORDS.has(t))
  );
}

function keywordOverlap(a: string, b: string): number {
  const ta = tokenize(a);
  const tb = tokenize(b);
  let shared = 0;
  for (const w of ta) if (tb.has(w)) shared++;
  return shared;
}

function pairId(a: string, b: string): string {
  return a < b ? `${a}|${b}` : `${b}|${a}`;
}

/**
 * Compute all connections for the current agent set.
 *
 * Complexity: O(N²) over agents with status === 'running' (and up to one
 * level beyond for goal-only links). For typical N ≤ 20 this is trivial.
 */
export function computeConnections(agents: Map<string, AgentState>): Connection[] {
  const list = [...agents.values()].filter(
    (a) => a.status === 'running' || a.status === 'queued'
  );
  const out: Map<string, Connection> = new Map();

  for (let i = 0; i < list.length; i++) {
    for (let j = i + 1; j < list.length; j++) {
      const a = list[i];
      const b = list[j];

      let strength = 0;
      const reasons: string[] = [];
      let phaseSync = false;

      // Signal 1: Same repo
      if (a.repo && b.repo && a.repo === b.repo) {
        strength += 0.55;
        reasons.push(`same repo (${a.repo})`);
      }

      // Signal 2: Same phase (only meaningful when there's a base connection)
      if (strength > 0 && a.phase === b.phase) {
        strength += 0.25;
        phaseSync = true;
        reasons.push(`both in ${a.phase}`);
      }

      // Signal 3: Goal keyword overlap
      const overlap = keywordOverlap(a.goal, b.goal);
      if (overlap >= 2) {
        strength += Math.min(0.2, overlap * 0.05);
        reasons.push(`${overlap} shared goal keywords`);
      } else if (overlap === 1 && strength > 0) {
        // Single keyword only counts if there's already a base signal
        strength += 0.05;
      }

      if (strength > 0 && reasons.length > 0) {
        const id = pairId(a.id, b.id);
        out.set(id, {
          id,
          fromId: a.id,
          toId: b.id,
          strength: Math.min(1, strength),
          reasons,
          phaseSync
        });
      }
    }
  }

  return [...out.values()];
}

/**
 * Find all connections involving a specific agent — used by the hover
 * tooltip to list peers.
 */
export function connectionsFor(
  connections: Connection[],
  agentId: string
): Connection[] {
  return connections.filter((c) => c.fromId === agentId || c.toId === agentId);
}
