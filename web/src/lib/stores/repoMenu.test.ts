/**
 * Repo dropdown rule tests — run with `npx tsx src/lib/stores/repoMenu.test.ts`.
 * Pure functions only: no store, no fetch mock, no timers.
 *
 * The golden section decodes the SAME
 * `crates/lopi-ui/tests/fixtures/repo_menu_golden.json` as the macOS port
 * (`macos/LopiTests/RepoMenuTests.swift`) and the Rust shape test
 * (`crates/lopi-ui/tests/repo_menu_golden.rs`). All three must agree — that
 * fixture is what stops the two surfaces drifting.
 */
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { repoOptions, type RepoEntry, NO_REMOTE_GROUP, AUTO_OPTION } from './repoMenu';
import { groupedMenu } from './optionMenu';
import type { Option } from './options';
import { eq, eqIs, ok, namedSummary } from '$lib/test-harness';

// ── optionMenu: today's ungrouped catalogs are the degenerate case ───────────
// Model/effort/branch/autonomy carry no `group`. They must come back as one flat
// pinned list and render exactly as they did before grouping existed.
const plain: Option[] = [
  { value: 'a', label: 'Alpha' },
  { value: 'b', label: 'Beta' }
];
const plainMenu = groupedMenu(plain);
eq(plainMenu.groups.length, 0, 'an ungrouped catalog produces no sections');
eq(plainMenu.pinned.length, 2, 'an ungrouped catalog pins every option');
eq(plainMenu.flat.length, 2, 'flat covers every row');
eqIs(plainMenu.pinned[0].index, 0, 'indices start at 0');
eqIs(plainMenu.pinned[1].index, 1, 'indices are render order');

// ── optionMenu: flat is indexed in RENDER order, not source order ────────────
// A grouped option appearing before an ungrouped one must not misindex the
// cursor: pinned rows render first regardless of where they sit in the source.
const mixed: Option[] = [
  { value: 'g', label: 'Grouped', group: 'G' },
  { value: 'p', label: 'Pinned' }
];
const mixedMenu = groupedMenu(mixed);
eqIs(mixedMenu.flat[0].value, 'p', 'pinned rows lead flat');
eqIs(mixedMenu.pinned[0].index, 0, 'the pinned row is cursor index 0');
eqIs(mixedMenu.groups[0].rows[0].index, 1, 'grouped rows follow');

// ── optionMenu: filtering matches label OR hint, case-insensitively ──────────
const hinted: Option[] = [
  { value: '/h/lopi', label: 'konjoai/lopi', hint: '/h/lopi', group: 'konjoai' },
  { value: '/h/other', label: 'konjoai/other', hint: '/h/other', group: 'konjoai' }
];
eq(groupedMenu(hinted, 'LOPI').flat.length, 1, 'query is case-insensitive');
eq(groupedMenu(hinted, '/h/other').flat.length, 1, 'a path fragment matches via the hint');
eq(groupedMenu(hinted, '  ').flat.length, 2, 'a whitespace-only query matches everything');
eq(groupedMenu(hinted, 'nope').groups.length, 0, 'a section with no matches disappears');

// ── repoMenu: the collision — the whole reason labels get a suffix ───────────
// A linked worktree and its main checkout report the same origin. They are two
// different run targets; they must not read as one row.
const collide: RepoEntry[] = [
  { path: '/h/squish', owner: 'konjoai', name: 'squish' },
  { path: '/h/squish-wt', owner: 'konjoai', name: 'squish' },
  { path: '/h/lopi', owner: 'konjoai', name: 'lopi' }
];
const collided = repoOptions(collide).filter((o) => o.value !== '');
const labels = collided.map((o) => o.label);
eq(new Set(labels).size, labels.length, 'every label is unique');
ok(labels.includes('konjoai/squish · squish'), 'an ambiguous label is suffixed with its directory');
ok(labels.includes('konjoai/squish · squish-wt'), 'both sides of a collision are suffixed');
ok(labels.includes('konjoai/lopi'), 'an UNambiguous label is left clean');
eq(
  collided.map((o) => o.value).sort().join(','),
  '/h/lopi,/h/squish,/h/squish-wt',
  'disambiguation never merges or drops a path'
);

// ── repoMenu: values are paths, always — the hard constraint ─────────────────
// `CreateTaskRequest.repo` reaches git2::Repository::open and lopi never clones,
// so a label is decoration and the path is the fact.
for (const o of collided) {
  eqIs(o.hint, o.value, 'the hint is the path, which is what makes path-search work');
  ok(o.value.startsWith('/'), 'the value stays an absolute path');
}
eqIs(AUTO_OPTION.value, '', 'auto is the empty no-override sentinel');
eqIs(AUTO_OPTION.group, undefined, 'auto is ungrouped, so it pins');

// ── repoMenu: a repo with no GitHub identity keeps its place ────────────────
const nameless = repoOptions([{ path: '/h/TinyStories', owner: null, name: 'TinyStories' }]);
eqIs(nameless[1].label, 'TinyStories', 'an unlabelled repo falls back to its name');
eqIs(nameless[1].group, NO_REMOTE_GROUP, 'and lands in the junk drawer');
eqIs(nameless[1].value, '/h/TinyStories', 'losing a label must never lose a repo');

// ── repoMenu: auto only pins while it matches ────────────────────────────────
// If auto pinned unconditionally it would sit at flat[0] while you type "lopi",
// and Enter would select auto instead of the row you are looking at.
const withAuto = repoOptions([{ path: '/h/lopi', owner: 'konjoai', name: 'lopi' }]);
eqIs(groupedMenu(withAuto, '').flat[0].value, '', 'auto leads an unfiltered menu');
eqIs(groupedMenu(withAuto, 'lopi').flat[0].value, '/h/lopi', 'auto steps aside under a query');
eq(groupedMenu(withAuto, 'lopi').pinned.length, 0, 'auto is gone when it does not match');

// ── The golden fixture — the cross-surface parity gate ───────────────────────
console.log('\n── golden repo-menu fixture ──────────────────────────');
const here = dirname(fileURLToPath(import.meta.url));
// ../../../../ — this file sits one deeper than `parser.test.ts` (lib/stores vs lib).
const goldenPath = resolve(here, '../../../../crates/lopi-ui/tests/fixtures/repo_menu_golden.json');
const golden = JSON.parse(readFileSync(goldenPath, 'utf8')) as {
  repos: RepoEntry[];
  options: { value: string; label: string; hint: string | null; group: string | null }[];
  filtered: Record<string, string[]>;
};

const built = repoOptions(golden.repos);
eq(built.length, golden.options.length, 'golden: option count');
golden.options.forEach((want, i) => {
  const got = built[i];
  eqIs(got.value, want.value, `golden[${i}] value`);
  eqIs(got.label, want.label, `golden[${i}] label — ${want.label}`);
  eqIs(got.hint ?? null, want.hint, `golden[${i}] hint`);
  eqIs(got.group ?? null, want.group, `golden[${i}] group`);
});

for (const [query, want] of Object.entries(golden.filtered)) {
  const got = groupedMenu(built, query).flat.map((o) => o.label);
  eq(got.join(' | '), want.join(' | '), `golden: filter ${JSON.stringify(query)}`);
}

namedSummary('repoMenu');
