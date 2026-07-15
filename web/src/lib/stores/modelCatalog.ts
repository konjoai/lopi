/**
 * Live Claude model/effort catalog, fetched once from `GET /api/models` (a
 * server-side proxy to Anthropic's real `/v1/models` — the browser never
 * calls Anthropic directly). Mirrors `branches.ts`'s per-repo cache pattern:
 * best-effort, one attempt cached whether it succeeds or fails. The static
 * `MODEL_OPTIONS`/`EFFORT_OPTIONS` in `options.ts` are the fallback shown
 * until the fetch lands (or forever, if it fails) — the same two-tier
 * fallback the backend's own `GET /api/models` handler already provides one
 * layer down (live Anthropic → server-side static list → this client-side
 * static list).
 */
import { writable } from 'svelte/store';
import { listModels } from '$lib/api';
import { MODEL_OPTIONS, EFFORT_OPTIONS, type Option } from './options';

/** One model as `GET /api/models` reports it. */
export interface ModelCatalogEntry {
  id: string;
  display_name: string;
  /** Reasoning-effort tiers this model supports, low-to-high. */
  effort: string[];
}

/** The live catalog once fetched; empty until `ensureModelCatalog` lands. */
export const modelCatalog = writable<ModelCatalogEntry[]>([]);

let inflight = false;
let attempted = false;

/** Fetch the live catalog once per page load. Safe to call from a reactive
 *  statement: repeat calls after the first attempt (success or failure) are
 *  no-ops, matching `ensureBranches`'s "one attempt, cache the failure too"
 *  posture — the endpoint itself never errors (it falls back server-side),
 *  so the only failure mode here is the request itself not completing
 *  (offline, server unreachable). */
export async function ensureModelCatalog(): Promise<void> {
  if (inflight || attempted) return;
  inflight = true;
  try {
    const { models } = await listModels();
    if (models.length) modelCatalog.set(models);
  } catch {
    // Leave the store empty — callers fall back to the static catalog.
  } finally {
    attempted = true;
    inflight = false;
  }
}

/** Model dropdown options — the live catalog if loaded, else the static
 *  fallback (which already carries the `auto` sentinel). The live catalog
 *  gets `auto` appended too, sourced from the static list, since Anthropic's
 *  `/v1/models` obviously has no concept of lopi's "no override" sentinel. */
export function modelOptionsFrom(catalog: ModelCatalogEntry[]): Option[] {
  if (!catalog.length) return MODEL_OPTIONS;
  const auto = MODEL_OPTIONS.find((o) => o.value === 'auto');
  const live = catalog.map((m) => ({ value: m.id, label: m.display_name }));
  return auto ? [...live, auto] : live;
}

/** Effort dropdown options for the currently-selected model: the live
 *  catalog's per-model tiers (relabeled via the static `EFFORT_OPTIONS`'s
 *  hint text where the tier name matches, so known tiers still read with
 *  real copy) if the model is in the live catalog, else the static fallback
 *  list unfiltered — covers both "catalog hasn't loaded yet" and "model is
 *  the `auto` sentinel, which names no real model to look up tiers for". */
export function effortOptionsFor(catalog: ModelCatalogEntry[], model: string): Option[] {
  const entry = catalog.find((m) => m.id === model);
  if (!entry) return EFFORT_OPTIONS;
  return entry.effort.map((tier) => {
    const known = EFFORT_OPTIONS.find((o) => o.value === tier);
    return known ?? { value: tier, label: tier.charAt(0).toUpperCase() + tier.slice(1) };
  });
}
