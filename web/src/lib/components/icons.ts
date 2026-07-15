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
  gauge: '<circle cx="12" cy="13" r="8"/><path d="M12 9v4l3 2M9 2h6"/>',
  list: '<path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01"/>',
  cpu: '<rect x="4" y="4" width="16" height="16" rx="2"/><path d="M4 10h16M10 4v16"/>',
  cron: '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
  loop: '<path d="M17 2l4 4-4 4"/><path d="M3 11v-1a4 4 0 0 1 4-4h14"/><path d="M7 22l-4-4 4-4"/><path d="M21 13v1a4 4 0 0 1-4 4H3"/>',
  layers: '<path d="M12 3l8 4-8 4-8-4 8-4z"/><path d="M4 11l8 4 8-4M4 15l8 4 8-4"/>',
  // The lopi mark (see `stacks/icons.ts`'s `ICONS.mark`) — hardcoded colors,
  // so it ignores this catalog's `fill="none" stroke="currentColor"` wrapper
  // on purpose: it's the brand logo, not a themeable glyph.
  mark: '<rect x="1.5" y="1.5" width="21" height="21" rx="5.25" fill="#0a0a0a"/><rect x="2.55" y="2.55" width="18.9" height="18.9" rx="4.5" fill="none" stroke="#ff9500" stroke-width="1.05" opacity="0.85"/><g transform="translate(3.9,3.9) scale(0.675)" fill="none" stroke="#ff9500" stroke-width="2.3" stroke-linecap="round" stroke-linejoin="round"><path d="M17 2l4 4-4 4"/><path d="M3 11v-1a4 4 0 0 1 4-4h14"/><path d="M7 22l-4-4 4-4"/><path d="M21 13v1a4 4 0 0 1-4 4H3"/></g>',
  wrench:
    '<path d="M14.7 6.3a4 4 0 0 1-5.4 5.4L4 17l3 3 5.3-5.3a4 4 0 0 1 5.4-5.4L14.7 12.7z"/>',
  logs: '<path d="M6 3h9l5 5v13H6z"/><path d="M15 3v5h5M8 12h8M8 16h8"/>',
  sliders: '<path d="M4 7h10M18 7h2M4 17h2M8 17h12"/><circle cx="16" cy="7" r="2.5"/><circle cx="6" cy="17" r="2.5"/>',
  bug: '<circle cx="12" cy="13" r="6"/><path d="M9 4h6M9 4l-1.5 3M15 4l1.5 3M6 13H3M21 13h-3M7 18l-2 2M17 18l2 2"/>'
} as const;

export type ShellIconKey = keyof typeof SHELL_ICONS;
