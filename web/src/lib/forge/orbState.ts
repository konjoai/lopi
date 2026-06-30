/**
 * orbState — the single derived description of how the living orb should look and
 * move for one session. The orb is driven entirely by this: color and motion are
 * pure functions of agent state, computed here so the mapping is testable and the
 * Forge component stays a dumb renderer.
 *
 * The phase→color source of truth stays `PHASE_COLORS` (see `agents.ts`); this
 * layers the terminal/blocked overrides and the motion parameters on top, exactly
 * per the ORB STATE MAP. Overrides (failed, cancelled, completed, awaiting,
 * rate-limited, rolling back) win over the plain phase coloring.
 */
import type { AgentState } from '$lib/stores/agents';
import { PHASE_COLORS } from '$lib/stores/phase-colors';

/** A non-color motion flourish the renderer special-cases. */
export type OrbSpecial =
  | 'none'
  | 'kryptonite' // bright jade halo: pulses 2–3× then settles, spin drifts down
  | 'hardStop' // no spin, hard steady rim (failed / error / cancelled)
  | 'reverseSpin' // agitated reverse rotation (rolling back / recovery)
  | 'stutter' // jittery spin (rate-limited / retry)
  | 'attentionPulse'; // gentle pulse while awaiting the user

/** Everything the orb renderer needs to draw one session's state. */
export interface OrbState {
  /** Hex glow / surface color. */
  glowColor: string;
  /** Spin rate, baseline 1.0; 0 = stopped (only on hardStop). */
  spinSpeed: number;
  /** Pulse-frequency multiplier, baseline 1.0. */
  pulseRate: number;
  /** Aura brightness, ~0.2 (idle) … ~1.4 (success bloom). */
  glowIntensity: number;
  /** Surface displacement intensity, 0 … 1. */
  turbulence: number;
  /** A named motion flourish layered on top. */
  special: OrbSpecial;
}

// ── Palette (mirrors web app.css orb-state tokens) ────────────────────────────
const C = {
  ice: '#00d4ff',
  iceDeep: '#0088aa',
  plasma: '#5ee6ff',
  violet: '#7c3aed',
  violetBright: '#9d5cff',
  mint: '#3be6c8',
  jade: '#00ff9d',
  ember: '#ff4500',
  flame: '#ff9500',
  sun: '#ffcc00',
  rose: '#ff0066',
  roseMuted: '#b04a6a'
} as const;

/** The calm orb shown when a pane holds no session. */
export const IDLE_ORB: OrbState = {
  glowColor: C.ice,
  spinSpeed: 0.25,
  pulseRate: 0.5,
  glowIntensity: 0.25,
  turbulence: 0.1,
  special: 'none'
};

/** True when the session is paused for a human (plan gate or a CLI prompt). */
function isAwaiting(agent: AgentState, awaiting: boolean): boolean {
  return awaiting || agent.awaitingApproval === true;
}

/** Phase-driven look while the agent is actively running (no override active). */
function runningOrb(agent: AgentState): OrbState {
  const act = clamp01(agent.activity ?? 0);
  const claude = (agent.claudePhase ?? '').toLowerCase();
  if (claude.includes('pr')) {
    return { glowColor: C.mint, spinSpeed: 1.4, pulseRate: 1.2, glowIntensity: 1.0, turbulence: 0.4, special: 'none' };
  }
  switch (agent.phase) {
    case 'Implementation':
      return { glowColor: C.plasma, spinSpeed: 1.6 + act, pulseRate: 1.4, glowIntensity: 1.2, turbulence: 0.9, special: 'none' };
    case 'Testing':
      return { glowColor: C.violet, spinSpeed: 1.3, pulseRate: 1.3, glowIntensity: 0.95, turbulence: 0.5, special: 'none' };
    case 'Conclusion':
      // Running + Conclusion = scoring / verifying (success is caught earlier).
      return { glowColor: C.violetBright, spinSpeed: 1.1, pulseRate: 1.2, glowIntensity: 0.95, turbulence: 0.35, special: 'none' };
    case 'Planning':
    case 'Discovery':
      return { glowColor: C.ice, spinSpeed: 0.9 + act * 0.6, pulseRate: 1.0, glowIntensity: 0.8, turbulence: 0.3, special: 'none' };
    default: // Boot
      return { glowColor: C.ice, spinSpeed: 0.6, pulseRate: 0.8, glowIntensity: 0.55, turbulence: 0.2, special: 'none' };
  }
}

/**
 * Compute the orb state for a session. `awaiting` is the externally-derived
 * permission-waiting flag (see `permissionWaiting` store); pass `false` if
 * unknown. A `null` agent yields the idle launcher orb.
 */
export function computeOrbState(agent: AgentState | null, awaiting = false): OrbState {
  if (!agent) return IDLE_ORB;

  // Terminal / blocked overrides win over the plain phase coloring.
  if (agent.taskStatus === 'RolledBack') {
    return { glowColor: C.ember, spinSpeed: 1.4, pulseRate: 1.4, glowIntensity: 1.0, turbulence: 0.8, special: 'reverseSpin' };
  }
  if (agent.status === 'failed') {
    return { glowColor: C.rose, spinSpeed: 0, pulseRate: 0, glowIntensity: 1.0, turbulence: 0.0, special: 'hardStop' };
  }
  if (agent.status === 'cancelled') {
    return { glowColor: C.roseMuted, spinSpeed: 0, pulseRate: 0, glowIntensity: 0.6, turbulence: 0.0, special: 'hardStop' };
  }
  if (agent.status === 'completed') {
    return { glowColor: C.jade, spinSpeed: 0.35, pulseRate: 0.8, glowIntensity: 1.4, turbulence: 0.2, special: 'kryptonite' };
  }
  if (isAwaiting(agent, awaiting)) {
    return { glowColor: C.sun, spinSpeed: 0.45, pulseRate: 0.7, glowIntensity: 0.9, turbulence: 0.25, special: 'attentionPulse' };
  }
  if (agent.throttled) {
    return { glowColor: C.flame, spinSpeed: 0.9, pulseRate: 1.2, glowIntensity: 0.9, turbulence: 0.4, special: 'stutter' };
  }
  if (agent.status === 'queued') {
    return { glowColor: C.iceDeep, spinSpeed: 0.5, pulseRate: 0.6, glowIntensity: 0.4, turbulence: 0.15, special: 'none' };
  }
  return runningOrb(agent);
}

/**
 * The phase color the orb resolves to with no override — kept exported so other
 * surfaces (rings, glows) can share the exact same source of truth.
 */
export function orbPhaseColor(agent: AgentState): string {
  return PHASE_COLORS[agent.phase] ?? C.ice;
}

function clamp01(n: number): number {
  if (!Number.isFinite(n)) return 0;
  return n < 0 ? 0 : n > 1 ? 1 : n;
}
