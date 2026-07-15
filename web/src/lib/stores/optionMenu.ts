/**
 * Grouping + filtering for `Dropdown.svelte`'s list — pure, and deliberately
 * policy-free.
 *
 * Order is *not* decided here: sections appear in the order their first option
 * appears in `options`, and options keep their given order within a section. The
 * caller sorts (`stores/repoMenu.ts`), which is what stops sections reshuffling
 * under a live filter — an order recomputed from *matching* counts would jump on
 * every keystroke.
 *
 * An option with no `group` pins above every section. So a catalog where nothing
 * carries a `group` — every field but `repo` — comes back as one flat `pinned`
 * list and renders exactly as it did before grouping existed. Today's dropdown
 * is the degenerate case of this function, not a branch around it.
 *
 * The macOS port is `macos/Lopi/Stacks/OptionMenu.swift`; the two surfaces must
 * agree row-for-row, which `repoMenu.test.ts` and `RepoMenuTests.swift` pin to
 * one shared golden fixture.
 */
import type { Option } from '$lib/stores/options';

/** An option plus its index into `OptionMenu.flat` — the index the keyboard
 *  cursor uses. Precomputed so the template never does index arithmetic across
 *  two nested loops. */
export interface MenuRow {
  opt: Option;
  index: number;
}

/** One section: the `group` key its options share, and its rows. */
export interface OptionGroup {
  key: string;
  rows: MenuRow[];
}

export interface OptionMenu {
  /** Ungrouped options, in their given order — rendered above every section. */
  pinned: MenuRow[];
  groups: OptionGroup[];
  /** Every selectable row, in render order. Section headers are absent, so a
   *  cursor walking this list steps over them for free. */
  flat: Option[];
}

/** Does `opt` survive `q` (already trimmed and lowercased)? Case-insensitive
 *  substring over the label and the hint. For repos the hint *is* the absolute
 *  path, so this one predicate is "match `owner/name` or the path" — with no
 *  second field to keep in sync across two languages. Exported so other
 *  filtered-option UIs (the goal input's `@repo` autocomplete —
 *  `repoMenu.ts::repoAutocomplete`) use the identical rule instead of a second
 *  copy that could drift. */
export function matches(opt: Option, q: string): boolean {
  return opt.label.toLowerCase().includes(q) || (opt.hint ?? '').toLowerCase().includes(q);
}

/** Partition `options` into pinned rows and ordered sections, keeping only what
 *  matches `query`. */
export function groupedMenu(options: Option[], query = ''): OptionMenu {
  const q = query.trim().toLowerCase();
  const passing = q ? options.filter((o) => matches(o, q)) : options;

  const pinnedOpts: Option[] = [];
  // A Map preserves insertion order, which is how sections end up in
  // first-appearance order without a second sort.
  const buckets = new Map<string, Option[]>();
  for (const opt of passing) {
    if (!opt.group) {
      pinnedOpts.push(opt);
      continue;
    }
    const bucket = buckets.get(opt.group);
    if (bucket) bucket.push(opt);
    else buckets.set(opt.group, [opt]);
  }

  // Index in render order (pinned, then each section) rather than source order —
  // the cursor walks what the user sees.
  let index = 0;
  const pinned = pinnedOpts.map((opt) => ({ opt, index: index++ }));
  const groups = [...buckets.entries()].map(([key, opts]) => ({
    key,
    rows: opts.map((opt) => ({ opt, index: index++ }))
  }));
  const flat = [...pinned, ...groups.flatMap((g) => g.rows)].map((r) => r.opt);

  return { pinned, groups, flat };
}
