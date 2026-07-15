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
import { matches } from '$lib/stores/optionMenu';

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

export interface RepoSuggestion {
  /** The full `@owner/name` token, ready to splice into the goal text. */
  token: string;
  label: string;
  hint: string;
  /** The resolved run target — always the absolute path (`Option.value`),
   *  never the decorative label. `selectRepo` writes this straight onto
   *  `card.config.repo`, so the card's stored repo is a path from the moment
   *  it's picked, not re-derived later by re-parsing the label back out of
   *  free text (see `parseComposerInput`'s repo-resolution doc comment). */
  value: string;
}

/** Filtered repo suggestions for the goal input's `@` autocomplete, given its
 *  *entire current value*. Only suggests while the *trailing* word in the
 *  goal text is a bare `@token` (`(?:^|\s)@(\S*)$`) — matches the grammar's
 *  `:alias "goal" @repo ×N` order, where `@repo` is typically the next thing
 *  typed right after the goal text, so (unlike the leading `:alias` token)
 *  this never needs to look at the cursor position: the match is always the
 *  end of the string, so "replace the match" and "replace the string's tail"
 *  are the same operation. Reuses `optionMenu.ts`'s `matches` predicate (label
 *  or hint, case-insensitive substring) so `@lopi` finds `konjoai/lopi` the
 *  same way the repo dropdown's own search box would. The `auto` sentinel
 *  (empty value) is never suggested — `@auto` names no real run target. */
export function repoAutocomplete(goalText: string, repoOptions: Option[]): RepoSuggestion[] {
  const match = /(?:^|\s)@(\S*)$/.exec(goalText);
  if (!match) return [];
  const q = match[1].toLowerCase();
  return repoOptions
    .filter((o) => o.value !== '' && matches(o, q))
    .map((o) => ({ token: `@${o.label}`, label: o.label, hint: o.hint ?? '', value: o.value }));
}

/** Resolve an `@`-token's parsed label (e.g. `"konjoai/lopi"`, as recovered by
 *  `parseComposerInput`'s `@(\S+)` grammar) back to its real path, by exact
 *  label match against the fetched catalog. Returns the input unresolved when
 *  no option matches — a stale/renamed repo, or free text typed by hand
 *  outside the autocomplete flow — so a value is never silently dropped, only
 *  left as a label `cardToTaskPayload` can't run (same as before this fix). */
export function resolveRepoToken(label: string, repoOptions: Option[]): string {
  return repoOptions.find((o) => o.value !== '' && o.label === label)?.value ?? label;
}

/** The inverse of `resolveRepoToken` — given a stored `config.repo` path,
 *  find its display label for the provenance chip. Falls back to the
 *  basename of the path (never the full absolute path, which is noisy UI)
 *  when the path isn't in the current catalog — e.g. a repo that's since
 *  been removed from disk. */
export function repoLabelForPath(path: string, repoOptions: Option[]): string {
  const found = repoOptions.find((o) => o.value === path);
  if (found) return found.label;
  const trimmed = path.replace(/\/+$/, '');
  return trimmed.slice(trimmed.lastIndexOf('/') + 1) || path;
}
