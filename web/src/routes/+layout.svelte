<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { init, connectionState } from '$lib/stores/agents';

  onMount(() => init());

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
</script>

<!-- Top bar — minimal, always visible -->
<header
  class="fixed top-0 inset-x-0 z-30 flex items-center justify-between px-6 py-3 bg-konjo-deep/80 backdrop-blur-md border-b border-white/5"
>
  <div class="flex items-center gap-3">
    <span class="font-display text-xl tracking-tight">lopi</span>
    <span class="font-mono text-[10px] uppercase tracking-widest opacity-50">· forge</span>
  </div>

  <div class="flex items-center gap-2 font-mono text-[11px]">
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
