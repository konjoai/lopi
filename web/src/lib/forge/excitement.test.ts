/**
 * Excitement envelope tests — runs as a standalone Node script.
 * Usage: `npx tsx src/lib/forge/excitement.test.ts` from web/
 */
import {
  EXCITE_DURATION_MS,
  smoothstep01,
  exciteLevel,
  shakeAmplitude,
  spinMultiplier,
  exciteColor,
  shakes
} from './excitement';
import { eqIs as eq, record, summary } from '$lib/test-harness';

function close(actual: number, expected: number, name: string, eps = 1e-9) {
  record(Math.abs(actual - expected) <= eps, `${name}: expected ≈${expected}, got ${actual}`);
}

// ── smoothstep01 ──────────────────────────────────────────────────────────────
eq(smoothstep01(0), 0, 'smoothstep at 0');
eq(smoothstep01(1), 1, 'smoothstep at 1');
close(smoothstep01(0.5), 0.5, 'smoothstep symmetric midpoint');
eq(smoothstep01(-3), 0, 'smoothstep clamps below');
eq(smoothstep01(7), 1, 'smoothstep clamps above');

// ── exciteLevel ───────────────────────────────────────────────────────────────
const now = 1_000_000;
eq(exciteLevel(0, now), 0, 'unset stimulus is calm');
eq(exciteLevel(-5, now), 0, 'negative stimulus is calm');
eq(exciteLevel(now, now), 1, 'fresh stimulus is fully excited');
close(
  exciteLevel(now - EXCITE_DURATION_MS / 2, now),
  0.5,
  'half-decayed at half duration'
);
eq(exciteLevel(now - EXCITE_DURATION_MS, now), 0, 'fully decayed at duration');
eq(exciteLevel(now - EXCITE_DURATION_MS * 4, now), 0, 'stale stimulus stays calm');
eq(exciteLevel(now + 500, now), 1, 'future stimulus (clock skew) treated as fresh');

// ── shakeAmplitude ────────────────────────────────────────────────────────────
eq(shakeAmplitude(0, 0.07), 0, 'no shake when calm');
close(shakeAmplitude(1, 0.07), 0.07, 'full shake at impact');
// Front-loading: at half excitement, shake is far below half amplitude.
const half = shakeAmplitude(0.5, 0.08);
eq(half < 0.04 / 2, true, 'shake is front-loaded (cubic)');

// ── spinMultiplier ────────────────────────────────────────────────────────────
eq(spinMultiplier(0, 9), 1, 'calm spin is baseline');
close(spinMultiplier(1, 9), 10, 'full excitement spins 10×');
const midSpin = spinMultiplier(0.5, 9);
eq(midSpin > 1 && midSpin < 10, true, 'mid excitement spins between bounds');

// ── exciteColor + shakes ──────────────────────────────────────────────────────
eq(exciteColor('request').join(','), '1,0.45,0.05', 'request flashes ember orange');
eq(exciteColor('success').join(','), '0,1,0.62', 'success blooms jade');
eq(exciteColor('failure').join(','), '1,0,0.4', 'failure flares rose');
eq(shakes('request'), true, 'request shakes');
eq(shakes('failure'), true, 'failure shakes');
eq(shakes('success'), false, 'success does not shake');

summary();
