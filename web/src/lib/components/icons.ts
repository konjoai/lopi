/**
 * App-shell icon catalog — the sidebar's nav glyphs plus the hamburger/close
 * controls. A separate module from `stacks/icons.ts` on purpose: the shell
 * is global chrome, not a stacks-feature concern, so it owns its own small
 * icon set rather than reaching into a feature folder.
 */
export const SHELL_ICONS = {
  menu: '<path d="M4 6h16M4 12h16M4 18h16"/>',
  close: '<path d="M18 6L6 18M6 6l12 12"/>',
  zap: '<path d="M13 2L3 14h7l-1 8 10-12h-7z"/>',
  grid: '<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>',
  network:
    '<circle cx="5" cy="6" r="2"/><circle cx="19" cy="6" r="2"/><circle cx="12" cy="18" r="2"/><path d="M6.7 7.3L11 16M17.3 7.3L13 16M7 6h10"/>',
  chart: '<path d="M3 12h4l2 8 4-16 2 8h6"/>',
  dollar: '<path d="M12 3v18"/><path d="M16.5 7.5c0-1.7-1.6-3-4.5-3s-5 1.3-5 3.5 2.1 3 5 3.5 5 1.3 5 3.5-2.1 3.5-5 3.5-4.5-1.3-4.5-3"/>',
  list: '<path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01"/>',
  cpu: '<rect x="4" y="4" width="16" height="16" rx="2"/><path d="M4 10h16M10 4v16"/>',
  cron: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  loop: '<path d="M17 2l4 4-4 4"/><path d="M3 11v-1a4 4 0 0 1 4-4h14"/><path d="M7 22l-4-4 4-4"/><path d="M21 13v1a4 4 0 0 1-4 4H3"/>',
  layers: '<path d="M12 3l8 4-8 4-8-4 8-4z"/><path d="M4 11l8 4 8-4M4 15l8 4 8-4"/>',
  // The lopi Loop Stacks mark (see `stacks/icons.ts`'s `ICONS.mark`) —
  // hardcoded colors, so it ignores this catalog's `fill="none"
  // stroke="currentColor"` wrapper on purpose: it's the brand logo, not a
  // themeable glyph. Two arced loop arrows above a three-bar stack that
  // fades toward the back. A nested `<svg>` with its own `viewBox` (not
  // just a `<g>`) since the design's native 52×52 coordinate space doesn't
  // match this catalog's 24×24 convention — nesting lets it establish its
  // own scale rather than requiring the path data to be hand-rescaled.
  mark: '<svg viewBox="0 0 52 52" fill="none" width="100%" height="100%"><path d="M12.5,15.5 V14 A6,6 0 0 1 18.5,8 H39.5" stroke="#ff9e12" stroke-width="2.85" stroke-linecap="round"/><polyline points="33.5,2 39.5,8 33.5,14" stroke="#ff9e12" stroke-width="2.85" stroke-linecap="round" stroke-linejoin="round" fill="none"/><path d="M39.5,18.5 V20 A6,6 0 0 1 33.5,26 H12.5" stroke="#ff9e12" stroke-width="2.85" stroke-linecap="round"/><polyline points="18.5,32 12.5,26 18.5,20" stroke="#ff9e12" stroke-width="2.85" stroke-linecap="round" stroke-linejoin="round" fill="none"/><rect x="8" y="34" width="36" height="4" rx="2" fill="#ff9e12" opacity="0.9"/><rect x="8" y="40" width="36" height="4" rx="2" fill="#ff9e12" opacity="0.65"/><rect x="8" y="46" width="36" height="4" rx="2" fill="#ff9e12" opacity="0.4"/></svg>',
  wrench:
    '<path d="M14.7 6.3a4 4 0 0 1-5.4 5.4L4 17l3 3 5.3-5.3a4 4 0 0 1 5.4-5.4L14.7 12.7z"/>',
  logs: '<path d="M6 3h9l5 5v13H6z"/><path d="M15 3v5h5M8 12h8M8 16h8"/>',
  sliders: '<path d="M4 7h10M18 7h2M4 17h2M8 17h12"/><circle cx="16" cy="7" r="2.5"/><circle cx="6" cy="17" r="2.5"/>',
  bug: '<circle cx="12" cy="13" r="6"/><path d="M9 4h6M9 4l-1.5 3M15 4l1.5 3M6 13H3M21 13h-3M7 18l-2 2M17 18l2 2"/>'
} as const;

export type ShellIconKey = keyof typeof SHELL_ICONS;
