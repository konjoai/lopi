<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { init, connectionState, stats } from '$lib/stores/agents';
  import { installKeyboardShortcuts, helpVisible } from '$lib/stores/keyboard';
  import HelpOverlay from '$lib/components/HelpOverlay.svelte';

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
  <div class="flex items-center gap-4">
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

  <div class="flex items-center gap-3 font-mono text-[11px]">
    <span class="opacity-50 tabular-nums">{$stats.running} live</span>
    <span class="opacity-20">·</span>
    <span
      class="w-1.5 h-1.5 rounded-full"
      style:background={indicatorColor($connectionState)}
      class:animate-pulse={$connectionState === 'connecting'}
    ></span>
    <span class="uppercase tracking-widest opacity-70">{indicatorLabel($connectionState)}</span>
  </div>
</header>

<!-- Push content below header -->
<main class="relative pt-12 min-h-screen z-10">
  <slot />
</main>

<!-- Global help overlay (toggle with ?) -->
<HelpOverlay />

<!-- Subtle hint at the bottom-right when help is hidden -->
{#if !$helpVisible}
  <button
    type="button"
    on:click={() => helpVisible.set(true)}
    class="fixed bottom-4 right-4 z-20 font-mono text-[10px] uppercase tracking-widest opacity-30 hover:opacity-70 transition-opacity bg-konjo-deep/60 backdrop-blur px-2.5 py-1 rounded border border-white/5"
  >
    press ? for shortcuts
  </button>
{/if}
