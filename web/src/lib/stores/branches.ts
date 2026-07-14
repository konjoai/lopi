/**
 * Per-repo branch cache backing the branch dropdowns.
 *
 * Keyed by repo rather than held as one flat list because a card's effective
 * repo is `card.config.repo ?? paneDefaults.repo` — two cards in one pane can
 * target two different repos, and each must offer its own repo's branches.
 *
 * Best-effort chrome, the same posture `listRepos` takes on `/stacks`: a failed
 * fetch caches an empty entry rather than throwing. Caching the failure is
 * deliberate — these are read from reactive statements that re-run on every
 * render, so an uncached miss would refetch in a loop. One attempt per repo per
 * page load; `resolveBranch` treats an empty list as "no knowledge" and leaves
 * the user's branch alone.
 */
import { get, writable } from 'svelte/store';
import { listBranches } from '$lib/api';

/** A repo's local branches plus its current HEAD (the preselect candidate). */
export interface RepoBranches {
  branches: string[];
  head: string;
}

/** Resolved repo path → its branches. Empty until `ensureBranches` lands. */
export const branchesByRepo = writable<Record<string, RepoBranches>>({});

/** Repos with a fetch in flight — a second caller must not race a duplicate. */
const inflight = new Set<string>();

/** Fetch `repo`'s branches once, then serve from cache. Safe to call from a
 *  reactive statement: repeat calls for a cached or in-flight repo are no-ops. */
export async function ensureBranches(repo: string): Promise<void> {
  if (inflight.has(repo) || get(branchesByRepo)[repo]) return;
  inflight.add(repo);
  try {
    const { branches, default: head } = await listBranches(repo);
    branchesByRepo.update((m) => ({ ...m, [repo]: { branches, head } }));
  } catch {
    branchesByRepo.update((m) => ({ ...m, [repo]: { branches: [], head: '' } }));
  } finally {
    inflight.delete(repo);
  }
}

/** A repo's branches as dropdown options — empty while uncached. */
export function branchOptionsFor(cache: Record<string, RepoBranches>, repo: string) {
  return (cache[repo]?.branches ?? []).map((b) => ({ value: b, label: b }));
}
