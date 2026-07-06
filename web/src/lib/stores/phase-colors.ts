/**
 * Phase → color source of truth, in a leaf module with no `$app`/Svelte imports
 * so it is importable from pure, browser-free code (the orb-state mapping, unit
 * tests) as well as the store. Mirrors the `--phase-*` vars in `app.css`.
 *
 * K-collision: Planning is realigned to ice and Testing recolored violet, since
 * yellow/orange is reserved for the awaiting-user orb state and green for
 * success — keeping the orb state map unambiguous.
 */
import type { Phase } from '$lib/types';

/** The accent color for each UI phase. */
export const PHASE_COLORS: Record<Phase, string> = {
  Boot: '#f5f5f5',
  Discovery: '#00d4ff',
  Planning: '#00d4ff', // K-collision: realigned to ice (yellow/green reserved)
  Implementation: '#ff4500',
  Testing: '#7c3aed', // K-collision: recolored violet (yellow = awaiting)
  Conclusion: '#00ff9d'
};
