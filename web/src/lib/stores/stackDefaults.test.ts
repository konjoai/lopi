/**
 * Branch-resolution tests — run with `npx tsx src/lib/stores/stackDefaults.test.ts`.
 * Pure function only: no store, no fetch mock, no timers.
 *
 * The macOS port of this rule (`macos/Lopi/Stacks/StackConfigTypes.swift`) has a
 * mirror of these cases in `LopiTests/StackBranchTests.swift` — the two surfaces
 * must agree on which branch a repo switch lands on.
 */
import { resolveBranch, SEED_BRANCH, DEFAULT_STACK_DEFAULTS } from './stackDefaults';
import { eqIs, namedSummary } from '$lib/test-harness';

const BRANCHES = ['main', 'dev', 'feat/x'];

// ── an explicit, still-valid choice always survives a repo switch ────────────
eqIs(resolveBranch('feat/x', BRANCHES, 'main'), 'feat/x', 'a valid explicit branch is kept');
eqIs(resolveBranch('main', BRANCHES, 'main'), 'main', 'a branch equal to HEAD is kept');

// ── unset or now-invalid falls back to the repo's HEAD ───────────────────────
eqIs(resolveBranch('', BRANCHES, 'dev'), 'dev', 'an unset branch adopts HEAD');
eqIs(resolveBranch('gone', BRANCHES, 'dev'), 'dev', 'a branch absent from the new repo adopts HEAD');

// ── HEAD itself can be unusable (detached HEAD → empty; stale → not in list) ─
eqIs(resolveBranch('', BRANCHES, ''), 'main', 'no HEAD falls back to the first branch');
eqIs(resolveBranch('gone', BRANCHES, 'also-gone'), 'main', 'an invalid HEAD falls back to the first branch');

// ── an empty list means "no knowledge", not "wipe it" ────────────────────────
// Unfetched or fetch-failed must never clobber what the user set: `branch`
// reaches the server as a planning constraint, so a silent wipe would relaunch
// against the wrong target.
eqIs(resolveBranch('feat/x', [], ''), 'feat/x', 'an unknown repo leaves an explicit branch alone');
eqIs(resolveBranch('', [], ''), '', 'an unknown repo leaves an unset branch unset');

// ── convergence: re-resolving a resolved value is a no-op, so the reactive
//    write-back in ConfigDrawer/StackConfigPopover cannot loop ───────────────
const once = resolveBranch('gone', BRANCHES, 'dev');
eqIs(resolveBranch(once, BRANCHES, 'dev'), once, 'resolveBranch is idempotent');

// ── the cold-start seed is a real default, not a phantom ─────────────────────
eqIs(DEFAULT_STACK_DEFAULTS.branch, SEED_BRANCH, 'a fresh stack seeds from SEED_BRANCH');

namedSummary('stackDefaults');
