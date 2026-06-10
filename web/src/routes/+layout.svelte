<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { init, connectionState, stats } from '$lib/stores/agents';
  import { installKeyboardShortcuts, helpVisible } from '$lib/stores/keyboard';
  import HelpOverlay from '$lib/components/HelpOverlay.svelte';
  import SessionSidebar from '$lib/components/SessionSidebar.svelte';

  let sidebarOpen = false;

  onMount(() => {
    init();
    installKeyboardShortcuts();
  });

  function indicatorColor(s: string): string {
    if (s === 'connected') return 'var(--konjo-jade)';
    if (s === 'mock') return 'var(--konjo-sun)';
    if (s === 'connecting') return 'var(--konjo-ice)';
    return 'var(--konjo-rose)';
  }
  function indicatorLabel(s: string): string {
    if (s === 'connected') return 'live';
    if (s === 'mock') return 'preview';
    if (s === 'connecting') return 'connecting';
    return 'offline';
  }

  $: pathname = $page.url.pathname;
  $: viewLabel = pathname.startsWith('/constellation') ? 'constellation' : 'forge';
</script>

<!-- Top bar — minimal, always visible. Houses navigation between views. -->
<header
  class="fixed top-0 inset-x-0 z-30 flex items-center justify-between px-6 py-3 bg-konjo-deep/80 backdrop-blur-md border-b border-white/5"
>
  <div class="flex items-center gap-3">
    <button
      type="button"
      on:click={() => (sidebarOpen = !sidebarOpen)}
      class="text-konjo-ice hover:bg-konjo-ice/10 w-9 h-9 rounded border border-konjo-ice/30 hover:border-konjo-ice flex items-center justify-center transition-colors"
      style:background={sidebarOpen ? 'rgba(0, 212, 255, 0.1)' : 'transparent'}
      title="Sessions"
      aria-label="Toggle sessions sidebar"
    >
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        width="18"
        height="18"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        aria-hidden="true"
      >
        <line x1="4" y1="7" x2="20" y2="7" />
        <line x1="4" y1="12" x2="20" y2="12" />
        <line x1="4" y1="17" x2="20" y2="17" />
      </svg>
    </button>
    <a href="/" class="font-display text-xl tracking-tight hover:text-konjo-ice transition-colors">
      lopi
    </a>
    <span class="font-mono text-[10px] uppercase tracking-widest opacity-50">· {viewLabel}</span>
  </div>

  <!-- View switcher — Forge ↔ Constellation -->
  <nav class="flex items-center gap-1 font-mono text-[11px]">
    <a
      href="/"
      class="px-3 py-1 rounded-md transition-colors uppercase tracking-widest"
      class:bg-white={false}
      class:text-konjo-ice={pathname === '/'}
      class:opacity-50={pathname !== '/'}
      style:background={pathname === '/' ? 'rgba(0, 212, 255, 0.08)' : 'transparent'}
    >
      Forge
    </a>
    <a
      href="/constellation"
      class="px-3 py-1 rounded-md transition-colors uppercase tracking-widest"
      class:text-konjo-ice={pathname.startsWith('/constellation')}
      class:opacity-50={!pathname.startsWith('/constellation')}
      style:background={pathname.startsWith('/constellation') ? 'rgba(0, 212, 255, 0.08)' : 'transparent'}
    >
      Constellation
    </a>
  </nav>

  <div class="flex items-center gap-4 font-mono text-[11px]">
    <button
      type="button"
      on:click={() => window.dispatchEvent(new CustomEvent('lopi:add-pane'))}
      class="text-konjo-ice hover:bg-konjo-ice/10 w-9 h-9 rounded transition-colors flex items-center justify-center text-2xl leading-none font-bold border border-konjo-ice/30 hover:border-konjo-ice"
      title="Add pane"
      aria-label="Add pane"
    >
      +
    </button>
    <span class="opacity-20">·</span>
    <span class="opacity-50 tabular-nums">{$stats.running} live</span>
    <span class="opacity-20">·</span>
    <span
      class="w-1.5 h-1.5 rounded-full"
      style:background={indicatorColor($connectionState)}
      class:animate-pulse={$connectionState === 'connecting'}
    ></span>
    <span class="uppercase tracking-widest opacity-70">{indicatorLabel($connectionState)}</span>
    <span class="opacity-20">·</span>
    <button
      type="button"
      on:click={() => helpVisible.set(!$helpVisible)}
      class="text-konjo-ice hover:bg-konjo-ice/10 px-2 py-1 rounded transition-colors"
      title="Help & Shortcuts"
    >
      ?
    </button>
  </div>
</header>

<!-- Sidebar + slot live side-by-side under the fixed header. The header is
     position:fixed so it doesn't take layout space; padding-top reserves the
     header strip and `height: 100vh` lets the row fill edge-to-edge. -->
<main class="flex relative z-10" style="height: 100vh; padding-top: 4rem; overflow: hidden;">
  <SessionSidebar bind:open={sidebarOpen} />
  <div class="flex-1 min-w-0 relative">
    <slot />
  </div>
</main>

<!-- Global help overlay (toggle with ?) -->
<HelpOverlay />
