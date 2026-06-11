<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { init, connectionState, stats } from '$lib/stores/agents';
  import { installKeyboardShortcuts, helpVisible } from '$lib/stores/keyboard';
  import { applyTheme } from '$lib/stores/theme';
  import HelpOverlay from '$lib/components/HelpOverlay.svelte';

  onMount(() => {
    applyTheme();
    init();
    installKeyboardShortcuts();
  });

  function indicatorColor(s: string): string {
    if (s === 'connected') return 'var(--konjo-jade)';
    if (s === 'mock') return 'var(--konjo-sun)';
    if (s === 'connecting') return 'var(--konjo-accent)';
    return 'var(--konjo-rose)';
  }
  function indicatorLabel(s: string): string {
    if (s === 'connected') return 'live';
    if (s === 'mock') return 'preview';
    if (s === 'connecting') return 'connecting';
    return 'offline';
  }

  // ── Tab definitions — OpenClaw Control UI parity, the Konjo way ────────────
  const tabs = [
    { href: '/', label: 'Forge' },
    { href: '/constellation', label: 'Constellation' },
    { href: '/tasks', label: 'Tasks' },
    { href: '/schedules', label: 'Schedules' },
    { href: '/logs', label: 'Logs' },
    { href: '/config', label: 'Config' },
    { href: '/debug', label: 'Debug' }
  ];

  function isActive(href: string, path: string): boolean {
    return href === '/' ? path === '/' : path.startsWith(href);
  }

  $: pathname = $page.url.pathname;
  $: activeTab = tabs.find((t) => isActive(t.href, pathname)) ?? tabs[0];
  // Immersive views own the full viewport (no page scroll); data tabs scroll.
  $: immersive = pathname === '/' || pathname.startsWith('/constellation') || pathname.startsWith('/onboard');
</script>

<!-- Top bar — minimal, always visible. Houses navigation between views. -->
<header
  class="fixed top-0 inset-x-0 z-30 flex items-center justify-between px-6 py-3 bg-konjo-deep/80 backdrop-blur-md border-b border-white/5"
>
  <div class="flex items-center gap-4 min-w-0">
    <a href="/" class="font-display text-xl tracking-tight hover:text-konjo-accent transition-colors">
      lopi
    </a>
    <span class="font-mono text-[10px] uppercase tracking-widest opacity-50 hidden sm:inline">
      · {activeTab.label.toLowerCase()}
    </span>
  </div>

  <!-- Tab bar -->
  <nav class="flex items-center gap-0.5 font-mono text-[11px] overflow-x-auto">
    {#each tabs as tab (tab.href)}
      {@const active = isActive(tab.href, pathname)}
      <a
        href={tab.href}
        class="relative px-3 py-1 rounded-md transition-all duration-200 uppercase tracking-widest whitespace-nowrap hover:text-konjo-accent"
        class:text-konjo-accent={active}
        class:opacity-50={!active}
        class:tab-active={active}
        style:background={active ? 'rgb(var(--konjo-accent-rgb) / 0.08)' : 'transparent'}
      >
        {tab.label}
      </a>
    {/each}
  </nav>

  <div class="flex items-center gap-4 font-mono text-[11px]">
    {#if pathname === '/'}
      <button
        type="button"
        on:click={() => window.dispatchEvent(new CustomEvent('lopi:add-pane'))}
        class="text-konjo-accent hover:bg-konjo-accent/10 px-2 py-1 rounded transition-colors"
        title="Add pane"
      >
        +
      </button>
      <span class="opacity-20">·</span>
    {/if}
    <span class="opacity-50 tabular-nums hidden md:inline">{$stats.running} live</span>
    <span class="opacity-20 hidden md:inline">·</span>
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
      class="text-konjo-accent hover:bg-konjo-accent/10 px-2 py-1 rounded transition-colors"
      title="Help & Shortcuts"
    >
      ?
    </button>
  </div>
</header>

<!-- Immersive views fill the viewport; data tabs get a scrollable canvas. -->
{#if immersive}
  <main class="relative pt-12 z-10" style="height: calc(100vh - 3rem); overflow: hidden;">
    <slot />
  </main>
{:else}
  <main class="relative pt-12 z-10 min-h-screen overflow-y-auto">
    <slot />
  </main>
{/if}

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

<style>
  /* Animated underline glow on the active tab */
  .tab-active::after {
    content: '';
    position: absolute;
    left: 0.75rem;
    right: 0.75rem;
    bottom: -2px;
    height: 1px;
    background: var(--konjo-accent);
    box-shadow: 0 0 8px var(--konjo-accent);
    animation: tab-glow 2.4s ease-in-out infinite;
  }
  @keyframes tab-glow {
    0%,
    100% {
      opacity: 0.55;
    }
    50% {
      opacity: 1;
    }
  }
</style>
