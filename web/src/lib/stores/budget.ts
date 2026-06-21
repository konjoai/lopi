/**
 * Budget governance — fleet spend, burn-rate, and a configurable hourly cap.
 *
 * The backend owns hard enforcement (budget_exceeded events); this layer gives
 * operators a live cost picture and a soft cap to watch against. `burnPerHour`
 * is the slope of fleet spend over a recent sampling window, extrapolated to an
 * hour, so the gauge reflects the *current* rate, not a session average.
 */
import { writable, derived, get, type Readable } from 'svelte/store';
import { agents } from './agents';

const CAP_KEY = 'lopi-budget-cap-v1';

function loadCap(): number {
  if (typeof localStorage === 'undefined') return 5;
  const v = Number(localStorage.getItem(CAP_KEY));
  return Number.isFinite(v) && v > 0 ? v : 5;
}

/** Soft hourly spend cap (USD/hour) the burn-rate is measured against. */
export const hourlyCap = writable<number>(loadCap());
hourlyCap.subscribe((v) => {
  if (typeof localStorage !== 'undefined') localStorage.setItem(CAP_KEY, String(v));
});

type Sample = { t: number; cost: number };
const samples = writable<Sample[]>([]);
let started = false;

/** Begin sampling fleet cost (idempotent). Call once from a long-lived view. */
export function startBudgetSampler(): void {
  if (started || typeof window === 'undefined') return;
  started = true;
  const tick = () => {
    let total = 0;
    for (const a of get(agents).values()) total += a.cost;
    samples.update((s) => [...s, { t: Date.now(), cost: total }].slice(-150));
  };
  tick();
  setInterval(tick, 2000);
}

export type BudgetState = 'ok' | 'warn' | 'over';

export interface FleetBudget {
  spent: number; // total USD burned this session
  running: number; // active agents
  burnPerHour: number; // current rate, USD/hour
  cap: number; // the hourly cap
  fraction: number; // burnPerHour / cap (can exceed 1)
  state: BudgetState;
  minutesToCap: number | null; // at current rate, mins until session spend = cap
}

/** Live fleet budget rollup driving the Budget view + per-pane meters. */
export const fleetBudget: Readable<FleetBudget> = derived(
  [agents, samples, hourlyCap],
  ([$agents, $samples, $cap]) => {
    let spent = 0;
    let running = 0;
    for (const a of $agents.values()) {
      spent += a.cost;
      if (a.status === 'running') running++;
    }

    let burnPerHour = 0;
    if ($samples.length >= 2) {
      const first = $samples[0];
      const last = $samples[$samples.length - 1];
      const dt = (last.t - first.t) / 1000;
      if (dt > 0) burnPerHour = Math.max(0, ((last.cost - first.cost) / dt) * 3600);
    }

    const fraction = $cap > 0 ? burnPerHour / $cap : 0;
    const state: BudgetState = fraction >= 1 ? 'over' : fraction >= 0.75 ? 'warn' : 'ok';
    const remaining = $cap - spent;
    const minutesToCap =
      burnPerHour > 0 && remaining > 0 ? (remaining / burnPerHour) * 60 : null;

    return { spent, running, burnPerHour, cap: $cap, fraction, state, minutesToCap };
  }
);

/** State → Konjo color, shared by the gauge, meters and pills. */
export function budgetColor(state: BudgetState): string {
  return state === 'over'
    ? 'var(--konjo-rose)'
    : state === 'warn'
      ? 'var(--konjo-flame)'
      : 'var(--konjo-jade)';
}
