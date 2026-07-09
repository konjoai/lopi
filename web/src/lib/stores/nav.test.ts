/**
 * Nav module tests — run with `npx tsx src/lib/stores/nav.test.ts`.
 * Pure logic only; no Svelte, no DOM.
 */
import { NAV_ITEMS, isActiveRoute, activeNavItem, isImmersiveRoute, SIDEBAR_MODE } from './nav';
import { eq, eqIs, ok, namedSummary } from '$lib/test-harness';

// ── NAV_ITEMS mirrors the pre-Shell-1 tab list, Forge moved to /forge ───────
{
  eq(
    NAV_ITEMS.map((i) => i.href),
    [
      '/forge',
      '/fleet',
      '/constellation',
      '/pulse',
      '/budget',
      '/tasks',
      '/router',
      '/schedules',
      '/loop',
      '/stacks',
      '/tools',
      '/logs',
      '/config',
      '/debug'
    ],
    'every previously-visible tab has a sidebar entry, in the same order, Forge relocated to /forge'
  );
  ok(
    NAV_ITEMS.every((i) => i.icon.trim().length > 0),
    'every nav item names a non-empty icon key'
  );
  ok(new Set(NAV_ITEMS.map((i) => i.href)).size === NAV_ITEMS.length, 'no duplicate hrefs');
  ok(!NAV_ITEMS.some((i) => i.href === '/'), 'root is never a nav destination — Forge owns /forge instead');
}

// ── isActiveRoute: exact + sub-route match, no false-positive prefix bleed ──
{
  ok(isActiveRoute('/loop', '/loop'), 'exact match is active');
  ok(isActiveRoute('/loop', '/loop/'), 'trailing-slash variant is active');
  ok(!isActiveRoute('/loop', '/loopback'), 'a same-prefix sibling route is NOT active (word-boundary check)');
  ok(!isActiveRoute('/logs', '/loop'), 'a different destination is not active');
  ok(isActiveRoute('/forge', '/forge'), 'forge is active on /forge now that it owns a real path');
}

// ── activeNavItem: resolves the matching item, undefined for unknown paths ─
{
  eqIs(activeNavItem('/stacks')?.label, 'Stacks', 'activeNavItem resolves an exact path');
  eqIs(activeNavItem('/tools')?.label, 'Tools', 'activeNavItem resolves another exact path');
  eqIs(activeNavItem('/nowhere'), undefined, 'an unknown path resolves to no active item');
}

// ── isImmersiveRoute: WebGL/full-viewport routes, Forge included post-move ─
{
  ok(isImmersiveRoute('/forge'), 'forge is immersive (moved from / but keeps the property)');
  ok(isImmersiveRoute('/fleet'), 'fleet is immersive');
  ok(isImmersiveRoute('/constellation'), 'constellation is immersive');
  ok(isImmersiveRoute('/onboard'), 'onboard is immersive (never a visible tab, still full-viewport)');
  ok(!isImmersiveRoute('/stacks'), 'stacks is a scrolling data view, not immersive');
  ok(!isImmersiveRoute('/loop'), 'loop is a scrolling data view, not immersive');
}

// ── SIDEBAR_MODE: the one-line closed-style switch defaults to hidden ──────
eqIs(SIDEBAR_MODE, 'hidden', 'the closed sidebar defaults to fully-hidden (off-canvas), not the icon-rail');

namedSummary('nav');
