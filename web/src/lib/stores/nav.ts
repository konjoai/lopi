/**
 * App-shell navigation — the single source of truth for sidebar nav items,
 * active-route matching, and which closed-sidebar visual style is live.
 * Split out of `+layout.svelte` so both are unit-testable without a DOM.
 */
import { writable } from 'svelte/store';

/** Whether the off-canvas sidebar is open. Shared between the topbar's
 *  hamburger button (in `+layout.svelte`) and `AppSidebar.svelte` (the
 *  panel/scrim/focus-trap owner) so either side can open or close it.
 *  Never persisted — every fresh load starts closed, per the brief. */
export const sidebarOpen = writable(false);

/** One sidebar destination. `icon` keys into `$lib/components/icons.ts`. */
export interface NavItem {
  href: string;
  label: string;
  icon: string;
}

/** Mirrors the tab list that lived inline in `+layout.svelte` before
 *  Shell-1, in the same order, with one change: Forge moved from `/` to
 *  `/forge` so `/` can redirect to `/stacks` (the new default view)
 *  without making Forge unreachable. `/onboard` was never a visible tab
 *  and stays that way. */
export const NAV_ITEMS: NavItem[] = [
  { href: '/forge', label: 'Forge', icon: 'zap' },
  { href: '/fleet', label: 'Fleet', icon: 'grid' },
  { href: '/constellation', label: 'Constellation', icon: 'network' },
  { href: '/pulse', label: 'Pulse', icon: 'chart' },
  { href: '/budget', label: 'Budget', icon: 'gauge' },
  { href: '/tasks', label: 'Tasks', icon: 'list' },
  { href: '/router', label: 'Router', icon: 'cpu' },
  { href: '/schedules', label: 'Schedules', icon: 'cron' },
  { href: '/loop', label: 'Loop', icon: 'loop' },
  { href: '/stacks', label: 'Stacks', icon: 'layers' },
  { href: '/tools', label: 'Tools', icon: 'wrench' },
  { href: '/logs', label: 'Logs', icon: 'logs' },
  { href: '/config', label: 'Config', icon: 'sliders' },
  { href: '/debug', label: 'Debug', icon: 'bug' }
];

/** A destination is active when `pathname` is it or a sub-route of it.
 *  No root (`/`) special-case is needed post-Shell-1 — every nav href is
 *  a real, non-root path now that Forge lives at `/forge`. */
export function isActiveRoute(href: string, pathname: string): boolean {
  return pathname === href || pathname.startsWith(`${href}/`);
}

/** The nav item (if any) matching `pathname`, for the topbar's "· label"
 *  breadcrumb. */
export function activeNavItem(pathname: string): NavItem | undefined {
  return NAV_ITEMS.find((item) => isActiveRoute(item.href, pathname));
}

/** Immersive routes own the full viewport (no page scroll, e.g. WebGL
 *  canvases); every other route gets a scrollable canvas. */
export function isImmersiveRoute(pathname: string): boolean {
  return (
    pathname.startsWith('/forge') ||
    pathname.startsWith('/fleet') ||
    pathname.startsWith('/constellation') ||
    pathname.startsWith('/onboard')
  );
}

/** The closed sidebar's visual style. `'hidden'` is fully off-canvas
 *  (`translateX(-100%)`, the Shell-1 default); `'rail'` is a persistent
 *  icon-only strip. The rail CSS ships in `AppSidebar.svelte` regardless
 *  of this value, unused while `'hidden'` is selected, so flipping this
 *  one constant is the entire migration — no rebuild. Not exposed as a
 *  user-facing toggle (out of scope this sprint). */
export const SIDEBAR_MODE: 'hidden' | 'rail' = 'hidden';
