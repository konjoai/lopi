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

/** The four-item nav — the whole of Unify-2 §5's collapse. Every earlier tab
 *  either merged into one of these (Fleet/Pulse/Dashboard/Tasks → Overview) or
 *  was cut outright (Constellation, Logs, Tools, Debug's sub-panels, Patterns,
 *  Router). `/onboard` was never a visible tab and stays that way; `/` still
 *  redirects to `/stacks` (Loop Stack, the default working surface).
 *
 *  - Loop Stack (`/stacks`): the single primary surface — StackPanes in the
 *    auto-tiling grid; a one-card pane reads like the old Forge box.
 *  - Scheduling (`/schedules`): cron/scheduling.
 *  - Overview (`/overview`): the read-only app-wide rollup that replaced
 *    Fleet + Dashboard + Pulse's information (Tasks' dead-letter is a filter
 *    within it).
 *  - Configuration (`/config`): app settings. */
export const NAV_ITEMS: NavItem[] = [
  { href: '/stacks', label: 'Loop Stack', icon: 'layers' },
  { href: '/schedules', label: 'Scheduling', icon: 'cron' },
  { href: '/overview', label: 'Overview', icon: 'list' },
  { href: '/config', label: 'Configuration', icon: 'sliders' }
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
    // The Loop Stack hosts its panes in the full-viewport auto-tiling grid
    // (Unify-2 §3), so it owns the whole canvas like the old Forge did.
    // `/onboard` is the only other full-viewport surface left after the cut.
    pathname.startsWith('/stacks') || pathname.startsWith('/onboard')
  );
}

/** The closed sidebar's visual style. `'hidden'` is fully off-canvas
 *  (`translateX(-100%)`, the Shell-1 default); `'rail'` is a persistent
 *  icon-only strip. The rail CSS ships in `AppSidebar.svelte` regardless
 *  of this value, unused while `'hidden'` is selected, so flipping this
 *  one constant is the entire migration — no rebuild. Not exposed as a
 *  user-facing toggle (out of scope this sprint). */
export const SIDEBAR_MODE: 'hidden' | 'rail' = 'hidden';
