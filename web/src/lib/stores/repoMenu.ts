/**
 * Repo dropdown policy: how a `/api/repos` entry becomes a labelled, grouped,
 * ordered `Option`. Pure — `stores/optionMenu.ts` does the grouping and decides
 * nothing about order; every decision below is made here, once.
 *
 * The macOS port is `macos/Lopi/Stacks/RepoMenu.swift`. Both are pinned to one
 * shared golden fixture (`crates/lopi-ui/tests/fixtures/repo_menu_golden.json`)
 * so the two surfaces cannot drift — the same mechanism `parser.test.ts` and
 * `AgentEventGoldenTests.swift` use for `agent_event_golden.json`.
 */
import type { Option } from '$lib/stores/options';

/** A repo as `GET /api/repos` reports it. `owner` is null when the checkout has
 *  no origin remote, or its origin is not GitHub. */
export interface RepoEntry {
  path: string;
  owner: string | null;
  name: string;
}

/** Section for repos with no GitHub identity. The space makes it uncollidable
 *  with a real owner — a GitHub login cannot contain one. */
export const NO_REMOTE_GROUP = 'no github remote';

/** The no-override sentinel. Ungrouped, so `groupedMenu` pins it above every
 *  section — but only while it matches the query, so that typing "lopi" doesn't
 *  leave `auto` sitting at `flat[0]` where Enter would select it. */
export const AUTO_OPTION: Option = { value: '', label: 'auto' };

/** The trailing path segment — the disambiguator when one `owner/name` covers
 *  two checkouts. */
function basename(path: string): string {
  const trimmed = path.replace(/\/+$/, '');
  return trimmed.slice(trimmed.lastIndexOf('/') + 1);
}

/** `owner/name`, or the bare name for a repo with no GitHub identity. */
function baseLabel(r: RepoEntry): string {
  return r.owner ? `${r.owner}/${r.name}` : r.name;
}

function groupOf(r: RepoEntry): string {
  return r.owner ?? NO_REMOTE_GROUP;
}

/** Three-way string compare. Explicit so the Swift port can mirror it exactly. */
function cmp(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

function tally(keys: string[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const k of keys) counts.set(k, (counts.get(k) ?? 0) + 1);
  return counts;
}

/**
 * Build the repo dropdown's options: `auto`, then every repo labelled
 * `owner/name`, grouped by owner, sorted so the sections a user works in most
 * come first.
 *
 * The value is always the absolute path — a path is the only thing that
 * identifies a run target (`CreateTaskRequest.repo` reaches
 * `git2::Repository::open`, and lopi never clones), so the label is decoration
 * and the path is the fact.
 *
 * Two checkouts can share one `owner/name`: a linked worktree and its main repo
 * both report the origin they share. Path labels can't collide; `owner/name`
 * labels can, and two different run targets rendering as one row is exactly the
 * failure this must not introduce — so an ambiguous label, and only an ambiguous
 * one, is suffixed with its directory name.
 */
export function repoOptions(repos: RepoEntry[]): Option[] {
  const bases = tally(repos.map(baseLabel));
  const counts = tally(repos.map(groupOf));

  const options: Option[] = repos.map((r) => {
    const base = baseLabel(r);
    const ambiguous = (bases.get(base) ?? 0) > 1;
    return {
      value: r.path,
      label: ambiguous ? `${base} · ${basename(r.path)}` : base,
      hint: r.path,
      group: groupOf(r)
    };
  });

  options.sort((a, b) => {
    const ga = a.group ?? '';
    const gb = b.group ?? '';
    if (ga !== gb) {
      // 1. The junk drawer sinks, however big it grows.
      const junkA = ga === NO_REMOTE_GROUP ? 1 : 0;
      const junkB = gb === NO_REMOTE_GROUP ? 1 : 0;
      if (junkA !== junkB) return junkA - junkB;
      // 2. Then the owners you have most checkouts of.
      const countA = counts.get(ga) ?? 0;
      const countB = counts.get(gb) ?? 0;
      if (countA !== countB) return countB - countA;
      // 3. Case-INSENSITIVE, else a capital's ASCII value puts `SteveFeldman`
      //    ahead of `bmaltais`. 4. then case-sensitive, to break exact ties.
      return cmp(ga.toLowerCase(), gb.toLowerCase()) || cmp(ga, gb);
    }
    // 5/6/7. Rows within a section. The chain ends on the path, which is unique
    //        — so the order is TOTAL, which is why JS's stable sort and Swift's
    //        unstable one produce the identical array. That's load-bearing, not
    //        decoration.
    return (
      cmp(a.label.toLowerCase(), b.label.toLowerCase()) ||
      cmp(a.label, b.label) ||
      cmp(a.value, b.value)
    );
  });

  return [AUTO_OPTION, ...options];
}
