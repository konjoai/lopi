/**
 * Per-repo real Claude Code `/name` command cache backing the composer's
 * `/`-triggered autocomplete (Composer-Grammar-2). Structurally identical to
 * `stores/branches.ts` — see that module's doc comment for the full
 * rationale (per-repo keying, best-effort caching of a failed fetch, one
 * attempt per repo per page load); this is the same shape for a different
 * per-repo catalog.
 */
import { get, writable } from 'svelte/store';
import { listClaudeCommands } from '$lib/api';
import type { Option } from '$lib/stores/controls';

/** One real Claude Code command/skill discovered in a repo. */
export interface ClaudeCommandOption {
  name: string;
  hint: string;
}

/** Resolved repo path → its discovered commands. Empty until
 *  `ensureClaudeCommands` lands. */
export const claudeCommandsByRepo = writable<Record<string, ClaudeCommandOption[]>>({});

/** Repos with a fetch in flight — a second caller must not race a duplicate. */
const inflight = new Set<string>();

/** Fetch `repo`'s Claude Code commands once, then serve from cache. Safe to
 *  call from a reactive statement: repeat calls for a cached or in-flight
 *  repo are no-ops. */
export async function ensureClaudeCommands(repo: string): Promise<void> {
  if (inflight.has(repo) || get(claudeCommandsByRepo)[repo]) return;
  inflight.add(repo);
  try {
    const { commands } = await listClaudeCommands(repo);
    claudeCommandsByRepo.update((m) => ({ ...m, [repo]: commands }));
  } catch {
    claudeCommandsByRepo.update((m) => ({ ...m, [repo]: [] }));
  } finally {
    inflight.delete(repo);
  }
}

/** A repo's commands as dropdown/autocomplete options — empty while
 *  uncached. `value`/`label` both carry the bare name (there is no
 *  separate display label, unlike a repo's `owner/name` vs. path); `hint`
 *  is the one-line description. */
export function claudeCommandOptionsFor(
  cache: Record<string, ClaudeCommandOption[]>,
  repo: string
): Option[] {
  return (cache[repo] ?? []).map((c) => ({ value: c.name, label: c.name, hint: c.hint || undefined }));
}
