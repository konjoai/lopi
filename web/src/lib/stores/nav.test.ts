/**
 * Nav module tests — run with `npx tsx src/lib/stores/nav.test.ts`.
 * Pure logic only; no Svelte, no DOM.
 */
import { NAV_ITEMS, isActiveRoute, activeNavItem, isImmersiveRoute, SIDEBAR_MODE } from './nav';
import { eq, eqIs, ok, namedSummary } from '$lib/test-harness';

// ── NAV_ITEMS is the four-item post-Unify-2 nav, in order ──────────────────
{
  eq(
    NAV_ITEMS.map((i) => i.href),
    ['/stacks', '/schedules', '/overview', '/config'],
    'the nav collapsed to exactly four: Loop Stack, Scheduling, Overview, Configuration'
  );
  eq(
    NAV_ITEMS.map((i) => i.label),
    ['Loop Stack', 'Scheduling', 'Overview', 'Configuration'],
    'the four labels, in order'
  );
  eqIs(NAV_ITEMS.length, 4, 'exactly four nav entries');
  // None of the ten cut destinations survives in the nav.
  const cut = ['/forge', '/fleet', '/constellation', '/pulse', '/tasks', '/router', '/logs', '/tools', '/debug'];
  ok(
    cut.every((href) => !NAV_ITEMS.some((i) => i.href === href)),
    'no cut route (Forge/Fleet/Constellation/Pulse/Tasks/Router/Logs/Tools/Debug) remains in the nav'
  );
  ok(
    NAV_ITEMS.every((i) => i.icon.trim().length > 0),
    'every nav item names a non-empty icon key'
  );
  ok(new Set(NAV_ITEMS.map((i) => i.href)).size === NAV_ITEMS.length, 'no duplicate hrefs');
  ok(!NAV_ITEMS.some((i) => i.href === '/'), 'root is never a nav destination — it redirects to /stacks');
}

// ── isActiveRoute: exact + sub-route match, no false-positive prefix bleed ──
{
  ok(isActiveRoute('/stacks', '/stacks'), 'exact match is active');
  ok(isActiveRoute('/stacks', '/stacks/'), 'trailing-slash variant is active');
  ok(!isActiveRoute('/stacks', '/stacksish'), 'a same-prefix sibling route is NOT active (word-boundary check)');
  ok(!isActiveRoute('/config', '/stacks'), 'a different destination is not active');
  ok(isActiveRoute('/overview', '/overview'), 'overview is active on its own path');
}

// ── activeNavItem: resolves the matching item, undefined for unknown paths ─
{
  eqIs(activeNavItem('/stacks')?.label, 'Loop Stack', 'activeNavItem resolves the Loop Stack');
  eqIs(activeNavItem('/overview')?.label, 'Overview', 'activeNavItem resolves Overview');
  eqIs(activeNavItem('/nowhere'), undefined, 'an unknown path resolves to no active item');
}

// ── isImmersiveRoute: only the surviving full-viewport surfaces ────────────
{
  ok(isImmersiveRoute('/stacks'), 'stacks (Loop Stack) is immersive — hosts the full-viewport pane grid');
  ok(isImmersiveRoute('/onboard'), 'onboard is immersive (never a visible tab, still full-viewport)');
  ok(!isImmersiveRoute('/overview'), 'overview is a scrolling data view, not immersive');
  ok(!isImmersiveRoute('/schedules'), 'scheduling is a scrolling data view, not immersive');
  ok(!isImmersiveRoute('/config'), 'configuration is a scrolling data view, not immersive');
}

// ── SIDEBAR_MODE: the one-line closed-style switch defaults to hidden ──────
eqIs(SIDEBAR_MODE, 'hidden', 'the closed sidebar defaults to fully-hidden (off-canvas), not the icon-rail');

namedSummary('nav');
