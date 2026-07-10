/**
 * Excitement envelope — the shared math behind the orb's "incoming request"
 * reaction (shake → fast spin → orange glow → settle).
 *
 * Every orb surface — the Forge centerpiece and the per-card OrbDots — consumes
 * this so the reaction reads as one visual language everywhere.
 */

/** How long a single stimulus burns, in milliseconds. */
export const EXCITE_DURATION_MS = 2500;

/** Hermite smoothstep on a clamped 0..1 input. */
export function smoothstep01(x: number): number {
  const t = x < 0 ? 0 : x > 1 ? 1 : x;
  return t * t * (3 - 2 * t);
}

/**
 * Raw envelope from a stimulus timestamp: 1.0 at the moment of impact,
 * linearly decaying to 0 over `EXCITE_DURATION_MS`. Returns 0 for unset
 * (`stimulus <= 0`) or stale stimuli.
 */
export function exciteLevel(stimulus: number, now: number): number {
  if (stimulus <= 0) return 0;
  const since = now - stimulus;
  if (since < 0) return 1; // clock skew — treat as fresh
  return Math.max(0, 1 - since / EXCITE_DURATION_MS);
}

/**
 * Shake amplitude for a given excitement level — cubed so the rattle is
 * front-loaded: violent on impact, settled long before the glow fades.
 */
export function shakeAmplitude(excite: number, scale: number): number {
  return excite * excite * excite * scale;
}

/** Spin multiplier — calm drift at 0, `1 + boost` at full excitement. */
export function spinMultiplier(excite: number, boost: number): number {
  return 1 + smoothstep01(excite) * boost;
}

/** What kind of event excited the orb — picks the reaction color. */
export type StimulusKind = 'request' | 'success' | 'failure';

/**
 * Reaction color per stimulus kind as a normalized RGB triple:
 * request → ember orange, success → jade bloom, failure → rose flare.
 */
export function exciteColor(kind: StimulusKind): [number, number, number] {
  switch (kind) {
    case 'success':
      return [0.0, 1.0, 0.62]; // konjo jade
    case 'failure':
      return [1.0, 0.0, 0.4]; // konjo rose
    default:
      return [1.0, 0.45, 0.05]; // ember orange
  }
}

/**
 * Whether a stimulus kind rattles the mesh. Requests and failures hit with
 * a physical shake; success blooms smoothly without one.
 */
export function shakes(kind: StimulusKind): boolean {
  return kind !== 'success';
}
