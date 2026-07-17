<script lang="ts">
  import '../app.css';
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { init, connectionState } from '$lib/stores/agents';
  import { installKeyboardShortcuts } from '$lib/stores/keyboard';
  import { applyTheme } from '$lib/stores/theme';
  import { budgetAlerts, dismissBudgetAlert } from '$lib/stores/events';
  import { activeNavItem, isImmersiveRoute, sidebarOpen } from '$lib/stores/nav';
  import HelpOverlay from '$lib/components/HelpOverlay.svelte';
  import AppSidebar from '$lib/components/AppSidebar.svelte';
  import LopiWordmark from '$lib/components/LopiWordmark.svelte';
  import { SHELL_ICONS } from '$lib/components/icons';

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
    if (s === 'connected') return 'online';
    if (s === 'mock') return 'preview';
    if (s === 'connecting') return 'connecting';
    return 'offline';
  }

  let hamburgerEl: HTMLButtonElement | undefined;

  $: pathname = $page.url.pathname;
  $: activeLabel = activeNavItem(pathname)?.label ?? '';
  // Immersive views own the full viewport (no page scroll); data tabs scroll.
  $: immersive = isImmersiveRoute(pathname);
</script>

<!-- Top bar — minimal, always visible. Hamburger opens the nav sidebar. -->
<header
  class="fixed top-0 inset-x-0 z-30 flex items-center justify-between px-6 py-3 bg-konjo-deep/80 backdrop-blur-md border-b border-white/5"
>
  <div class="flex items-center gap-4 min-w-0">
    <button
      type="button"
      bind:this={hamburgerEl}
      on:click={() => sidebarOpen.set(!$sidebarOpen)}
      aria-label="Toggle navigation"
      aria-expanded={$sidebarOpen}
      class="press w-8 h-8 flex items-center justify-center rounded-md border border-white/10 text-white/50 hover:text-konjo-flame hover:border-konjo-flame/40 transition-colors flex-shrink-0"
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" class="w-[18px] h-[18px]">
        {@html SHELL_ICONS.menu}
      </svg>
    </button>
    <a href="/stacks" class="font-display tracking-tight hover:text-konjo-accent transition-colors">
      <LopiWordmark size={20} />
    </a>
    <span class="font-mono text-[10px] uppercase tracking-widest opacity-50 hidden sm:inline">
      · {activeLabel.toLowerCase()}
    </span>
  </div>

  <div class="flex items-center gap-4 font-mono text-[11px]">
    <span
      class="w-1.5 h-1.5 rounded-full"
      style:background={indicatorColor($connectionState)}
      class:animate-pulse={$connectionState === 'connecting'}
    ></span>
    <span class="uppercase tracking-widest opacity-70">{indicatorLabel($connectionState)}</span>
    {#if pathname.startsWith('/stacks')}
      <span class="opacity-20">·</span>
      <button
        type="button"
        on:click={() => window.dispatchEvent(new CustomEvent('lopi:add-pane'))}
        aria-label="Add pane"
        class="press w-8 h-8 flex items-center justify-center rounded-md border border-konjo-flame/40 bg-konjo-flame/15 text-konjo-flame text-base leading-none hover:bg-konjo-flame/25 hover:border-konjo-flame/60 transition-colors flex-shrink-0"
        title="Add pane"
      >
        +
      </button>
    {/if}
  </div>
</header>

<AppSidebar triggerEl={hamburgerEl} />

<!-- Immersive views fill the viewport; data tabs get a scrollable canvas. -->
{#if immersive}
  <main class="relative pt-12 z-10" style="height: 100vh; overflow: hidden;">
    <slot />
  </main>
{:else}
  <main class="relative pt-12 z-10 min-h-screen overflow-y-auto">
    <slot />
  </main>
{/if}

<!-- Budget breach toasts — live alerts from the event stream -->
{#if $budgetAlerts.length > 0}
  <div class="fixed top-16 right-4 z-40 flex flex-col gap-2 w-80 max-w-[calc(100vw-2rem)]">
    {#each $budgetAlerts as alert (alert.seq)}
      <div class="budget-toast rounded-lg border border-konjo-rose/40 bg-konjo-deep/95 backdrop-blur-md p-3 shadow-2xl">
        <div class="flex items-start gap-2">
          <span class="text-konjo-rose text-lg leading-none mt-0.5">◈</span>
          <div class="flex-1 min-w-0">
            <div class="font-display text-sm font-bold text-konjo-rose">
              Budget exceeded · {alert.scope}
            </div>
            <div class="font-mono text-[11px] opacity-70 mt-0.5">
              ${alert.burnedUsd.toFixed(2)} burned against a ${alert.limitUsd.toFixed(2)}/h cap
              {#if alert.taskId}· task {alert.taskId.slice(0, 8)}{/if}
            </div>
          </div>
          <button
            type="button"
            on:click={() => dismissBudgetAlert(alert.seq)}
            class="w-4 h-4 flex items-center justify-center text-white/40 hover:text-white text-[10px] flex-shrink-0"
            aria-label="Dismiss"
          >
            ✕
          </button>
        </div>
      </div>
    {/each}
  </div>
{/if}

<!-- Global help overlay (toggle with ?) -->
<HelpOverlay />

<style>
  /* Budget toast — slide in from the right with a brief shake on entry */
  .budget-toast {
    animation:
      toast-in 0.4s cubic-bezier(0.16, 1, 0.3, 1) both,
      toast-shake 0.5s ease-in-out 0.4s;
  }
  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translateX(24px);
    }
    to {
      opacity: 1;
      transform: translateX(0);
    }
  }
  @keyframes toast-shake {
    0%,
    100% {
      transform: translateX(0);
    }
    25% {
      transform: translateX(-3px);
    }
    75% {
      transform: translateX(3px);
    }
  }
</style>
