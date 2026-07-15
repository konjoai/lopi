<!--
  AppSidebar — the off-canvas nav shell (Shell-1). Owns the sidebar panel,
  the scrim, keyboard/focus behavior, and nothing else — the hamburger
  button that opens it lives in `+layout.svelte`'s topbar and shares state
  via `stores/nav.ts::sidebarOpen`, since it's visually part of the topbar,
  not this panel.

  Closed by default every load (`sidebarOpen` starts `false`, never
  persisted). Closes on scrim-click, Escape, or selecting a nav item.
  Traps focus within the panel while open; returns focus to `triggerEl`
  (the hamburger button) on close. `prefers-reduced-motion` disables the
  slide transition via CSS only — no JS branching needed.
-->
<script lang="ts">
  import { tick } from 'svelte';
  import { page } from '$app/stores';
  import { NAV_ITEMS, isActiveRoute, sidebarOpen, SIDEBAR_MODE } from '$lib/stores/nav';
  import { SHELL_ICONS, type ShellIconKey } from './icons';
  import LopiWordmark from './LopiWordmark.svelte';

  /** The hamburger button that opens this sidebar — focus returns here on
   *  close. `null` is tolerated (focus just stays wherever it was). */
  export let triggerEl: HTMLElement | null = null;

  function iconMarkup(key: string): string {
    return SHELL_ICONS[key as ShellIconKey] ?? '';
  }

  let panelEl: HTMLElement | undefined;

  $: pathname = $page.url.pathname;
  $: rail = SIDEBAR_MODE === 'rail';

  function close(): void {
    sidebarOpen.set(false);
    triggerEl?.focus();
  }

  function onKeydown(e: KeyboardEvent): void {
    if (!$sidebarOpen) return;
    if (e.key === 'Escape') {
      close();
      return;
    }
    if (e.key !== 'Tab' || !panelEl) return;
    const focusables = panelEl.querySelectorAll<HTMLElement>('a, button');
    if (focusables.length === 0) return;
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }

  // Move focus into the panel the moment it opens, for keyboard users.
  // `tick()` waits for the `inert` attribute to actually clear from the DOM
  // first — focusing an element that's still inert is a silent no-op.
  $: if ($sidebarOpen) focusFirst();
  async function focusFirst(): Promise<void> {
    await tick();
    const target = panelEl?.querySelector<HTMLElement>('a, button');
    target?.focus();
  }
</script>

<svelte:window on:keydown={onKeydown} />

<!-- Decorative click-catcher, not a focus target — Esc and the panel's own
     close button are the keyboard-equivalent ways to dismiss. -->
<div class="scrim" class:on={$sidebarOpen} aria-hidden="true" on:click={close}></div>

<nav
  class="sidebar"
  class:open={$sidebarOpen}
  class:rail
  bind:this={panelEl}
  aria-label="Main navigation"
  inert={!$sidebarOpen}
>
  <div class="shead">
    <span class="brand"><LopiWordmark size={16} /></span>
    <button type="button" class="sclose" on:click={close} aria-label="Close navigation">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        {@html SHELL_ICONS.close}
      </svg>
    </button>
  </div>
  <div class="snav">
    {#each NAV_ITEMS as item (item.href)}
      {@const active = isActiveRoute(item.href, pathname)}
      <a href={item.href} class:active on:click={close}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          {@html iconMarkup(item.icon)}
        </svg>
        <span>{item.label}</span>
      </a>
    {/each}
  </div>
</nav>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    z-index: 38;
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.22s ease;
  }
  .scrim.on {
    opacity: 1;
    pointer-events: auto;
  }

  .sidebar {
    position: fixed;
    top: 0;
    left: 0;
    bottom: 0;
    width: 250px;
    z-index: 39;
    background: var(--konjo-panel, #0a0d0f);
    border-right: 1px solid rgba(255, 255, 255, 0.11);
    transform: translateX(-100%);
    transition: transform 0.24s cubic-bezier(0.16, 1, 0.3, 1);
    display: flex;
    flex-direction: column;
    box-shadow: 24px 0 60px rgba(0, 0, 0, 0.5);
  }
  .sidebar.open {
    transform: translateX(0);
  }

  /* Icon-rail variant — CSS-only, gated behind SIDEBAR_MODE. Unused while
     the default ('hidden') is selected; flipping that one constant makes
     this the closed (not just open) resting state instead. */
  .sidebar.rail {
    width: 64px;
  }
  .sidebar.rail .shead .brand,
  .sidebar.rail .snav a span {
    display: none;
  }
  .sidebar.rail .snav a {
    justify-content: center;
    padding: 10px 0;
  }

  .shead {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 16px 18px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  }
  .shead .brand {
    font-family: var(--font-mono, monospace);
    font-size: 16px;
    color: var(--konjo-paper, #f5f5f5);
  }
  .sclose {
    margin-left: auto;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.11);
    background: transparent;
    color: rgba(245, 245, 245, 0.28);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
  }
  .sclose:hover {
    color: var(--konjo-rose);
    border-color: rgba(255, 45, 120, 0.4);
  }
  .sclose :global(svg) {
    width: 14px;
    height: 14px;
  }

  .snav {
    flex: 1;
    overflow-y: auto;
    padding: 10px 10px 18px;
  }
  .snav a {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-radius: 8px;
    font-family: var(--font-mono, monospace);
    font-size: 12.5px;
    letter-spacing: 0.02em;
    color: rgba(245, 245, 245, 0.66);
    text-decoration: none;
    margin-bottom: 2px;
    transition: background-color 0.12s, color 0.12s;
    border: 1px solid transparent;
  }
  .snav a svg {
    width: 16px;
    height: 16px;
    flex: 0 0 auto;
    color: rgba(245, 245, 245, 0.28);
    transition: color 0.12s;
  }
  .snav a:hover {
    background: rgba(255, 255, 255, 0.05);
    color: var(--konjo-paper, #f5f5f5);
  }
  .snav a:hover svg {
    color: var(--konjo-flame);
  }
  .snav a.active {
    background: rgba(255, 149, 0, 0.09);
    color: var(--konjo-flame);
    border-color: rgba(255, 149, 0, 0.25);
  }
  .snav a.active svg {
    color: var(--konjo-flame);
  }

  @media (prefers-reduced-motion: reduce) {
    .sidebar,
    .scrim {
      transition: none;
    }
  }
</style>
