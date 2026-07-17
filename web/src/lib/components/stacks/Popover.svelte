<!--
  Popover — shared floating-panel primitive for the schedule/guardrails/evals
  facet popovers. Floats near its trigger with a tail, flips above when the
  viewport is too short below, clamps into the viewport horizontally, closes
  on outside-click / Escape / page scroll, and collapses into a bottom sheet
  under 520px. Only one Popover is ever open across the whole app — every
  instance shares `activePopoverId`, so opening a new one closes whatever
  else was open.
-->
<script context="module" lang="ts">
  import { writable } from 'svelte/store';

  /** The `id` of the currently-open popover, or `null`. Shared module state
   *  is what makes "one popover open at a time" trivial: every instance
   *  renders iff its own id matches. */
  export const activePopoverId = writable<string | null>(null);

  /** Open `id`, closing whatever else was open. Calling it again on the
   *  same id closes it (toggle-from-trigger behavior). */
  export function togglePopover(id: string): void {
    activePopoverId.update((cur) => (cur === id ? null : id));
  }

  export function closePopover(): void {
    activePopoverId.set(null);
  }
</script>

<script lang="ts">
  import { onDestroy, tick } from 'svelte';

  /** Stable identity for this popover instance, e.g. `${card.id}:sched`. */
  export let id: string;
  /** The trigger element to float near and flip/clamp against. */
  export let anchor: HTMLElement | null;
  /** Drives the header/footer/tail accent color. At loop scope config is an
   *  inline drawer, not a popover — `'config'` exists for the stack control
   *  dock's default-config popover (Stack-1), which has no inline-drawer
   *  equivalent since there's no card to expand underneath. */
  export let kind: 'sched' | 'guard' | 'eval' | 'config' | 'max' | 'goal' = 'sched';

  $: open = $activePopoverId === id;

  let popEl: HTMLDivElement | undefined;
  let left = 0;
  let top = 0;
  let tailSide: 'left' | 'right' = 'left';
  let flipped = false;
  let isSheet = false;
  let positioned = false;

  async function computePosition() {
    if (!open) return;
    isSheet = typeof window !== 'undefined' && window.innerWidth < 520;
    if (isSheet) {
      positioned = true;
      return;
    }
    // Wait for the DOM to catch up: on first open, `popEl`'s `bind:this`
    // hasn't committed yet when this reactive call fires, so the "is it
    // even mounted" check must happen *after* this tick, not before.
    await tick();
    if (!anchor || !popEl) {
      positioned = true;
      return;
    }
    const r = anchor.getBoundingClientRect();
    const pw = popEl.offsetWidth;
    const ph = popEl.offsetHeight;
    let l = r.left + r.width / 2 - 24;
    let t = r.bottom + 11;
    tailSide = 'left';
    flipped = false;
    if (l + pw > window.innerWidth - 10) {
      l = window.innerWidth - pw - 10;
      tailSide = 'right';
    }
    if (t + ph > window.innerHeight - 10) {
      t = r.top - ph - 11;
      flipped = true;
    }
    left = Math.max(10, l);
    top = Math.max(10, t);
    positioned = true;
  }

  function onOutside(e: MouseEvent) {
    if (!open) return;
    const target = e.target as Node;
    if (popEl?.contains(target)) return;
    if (anchor?.contains(target)) return;
    closePopover();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') closePopover();
  }

  function onScroll() {
    closePopover();
  }

  $: if (open) {
    positioned = false;
    computePosition();
  }

  // Stack-Chain-1 kill-test finding: `computePosition` only re-ran on open or
  // window resize, never when the popover's OWN content grew after opening
  // (e.g. toggling "run on a schedule" mounts a taller cron builder inside
  // the same popover). It correctly flipped above for the small initial
  // content, then never repositioned once the content grew past the
  // viewport bottom — a stale-measurement bug, not a "no room above" policy
  // gap. A `ResizeObserver` on `popEl` re-runs the same flip/clamp logic
  // whenever its content box actually changes size, fixing that directly.
  let resizeObserver: ResizeObserver | undefined;
  $: if (popEl) {
    resizeObserver?.disconnect();
    resizeObserver = new ResizeObserver(() => computePosition());
    resizeObserver.observe(popEl);
  } else {
    resizeObserver?.disconnect();
    resizeObserver = undefined;
  }

  onDestroy(() => {
    resizeObserver?.disconnect();
    if ($activePopoverId === id) closePopover();
  });
</script>

<svelte:window on:resize={computePosition} on:keydown={onKeydown} on:scroll|capture={onScroll} />
<svelte:body on:mousedown|capture={onOutside} />

{#if open}
  {#if isSheet}
    <div class="sheet-scrim" on:click={closePopover} on:keydown={() => {}} role="presentation"></div>
    <div
      class="pop {kind} sheet"
      class:positioned
      bind:this={popEl}
      role="dialog"
      aria-label="{kind} settings"
    >
      <slot />
    </div>
  {:else}
    <div
      class="pop {kind}"
      class:tailRight={tailSide === 'right'}
      class:tailLeft={tailSide === 'left'}
      class:flipped
      class:positioned
      style="left:{left}px;top:{top}px"
      bind:this={popEl}
      role="dialog"
      aria-label="{kind} settings"
    >
      <slot />
    </div>
  {/if}
{/if}

<style>
  .sheet-scrim {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    z-index: 55;
  }
  .pop {
    position: fixed;
    background: var(--konjo-panel, #0a0d0f);
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 9px;
    box-shadow: 0 18px 50px rgba(0, 0, 0, 0.75);
    width: 300px;
    max-width: calc(100vw - 20px);
    overflow: hidden;
    z-index: 60;
    visibility: hidden;
  }
  .pop.positioned {
    visibility: visible;
  }
  .pop::before {
    content: '';
    position: absolute;
    top: -7px;
    width: 12px;
    height: 12px;
    background: var(--konjo-panel, #0a0d0f);
    border-left: 1px solid rgba(255, 255, 255, 0.11);
    border-top: 1px solid rgba(255, 255, 255, 0.11);
    transform: rotate(45deg);
  }
  .pop.tailLeft::before {
    left: 22px;
  }
  .pop.tailRight::before {
    right: 22px;
  }
  .pop.flipped::before {
    top: auto;
    bottom: -7px;
    transform: rotate(225deg);
  }
  .pop.sheet {
    left: 0;
    right: 0;
    bottom: 0;
    top: auto;
    width: 100%;
    max-width: 100%;
    border-radius: 14px 14px 0 0;
  }
  .pop.sheet::before {
    content: none;
  }
  .pop :global(.ph) {
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    padding: 11px 13px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .pop :global(.ph svg) {
    width: 12px;
    height: 12px;
  }
  .pop.sched :global(.ph) {
    color: var(--konjo-ice);
  }
  .pop.guard :global(.ph) {
    color: var(--konjo-sun);
  }
  .pop.eval :global(.ph) {
    color: var(--konjo-jade);
  }
  .pop.config :global(.ph) {
    color: var(--stack-violet, #b79bff);
  }
  .pop.max :global(.ph) {
    color: var(--konjo-flame);
  }
  /* Explicit size — an earlier draft left this SVG unsized and it rendered
     at the browser's ~300px intrinsic default. */
  .pop.max :global(.ph svg) {
    width: 13px;
    height: 13px;
  }
  .pop.goal :global(.ph) {
    color: var(--konjo-flame);
  }
  .pop :global(.pbody) {
    padding: 11px 13px;
    max-height: 56vh;
    overflow-y: auto;
  }
  .pop :global(.popfoot) {
    padding: 9px 13px;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 9px;
  }
  .pop :global(.apply) {
    border-radius: 7px;
    padding: 7px 18px;
    font-family: var(--font-mono, monospace);
    font-size: 10.5px;
    font-weight: 700;
    cursor: pointer;
    border: 1px solid;
    box-shadow:
      0 2px 5px rgba(0, 0, 0, 0.4),
      0 1px 0 rgba(255, 255, 255, 0.04) inset;
    transition: none;
  }
  .pop.sched :global(.apply) {
    background: rgba(0, 212, 255, 0.15);
    color: var(--konjo-ice);
    border-color: rgba(0, 212, 255, 0.5);
  }
  .pop.guard :global(.apply) {
    background: rgba(255, 204, 0, 0.15);
    color: var(--konjo-sun);
    border-color: rgba(255, 204, 0, 0.5);
  }
  .pop.eval :global(.apply) {
    background: rgba(0, 255, 157, 0.15);
    color: var(--konjo-jade);
    border-color: rgba(0, 255, 157, 0.5);
  }
  .pop.config :global(.apply) {
    background: rgba(183, 155, 255, 0.15);
    color: var(--stack-violet, #b79bff);
    border-color: rgba(183, 155, 255, 0.5);
  }
  .pop.max :global(.apply) {
    background: rgba(255, 149, 0, 0.15);
    color: var(--konjo-flame);
    border-color: rgba(255, 149, 0, 0.4);
  }
  .pop.goal :global(.apply) {
    background: rgba(255, 149, 0, 0.15);
    color: var(--konjo-flame);
    border-color: rgba(255, 149, 0, 0.4);
  }
  @media (prefers-reduced-motion: reduce) {
    .pop {
      transition: none;
    }
  }
</style>
