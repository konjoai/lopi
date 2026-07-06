<!--
  ForgeStage — the living orb's home inside the agent pane.

  Idle (no session): the orb sits large and centered as the launcher, with the
  controls slotted beneath it. The moment a session goes live the orb is absorbed
  into the bottom-right corner — it visibly travels and shrinks in one spring
  (a FLIP: measure first/last rects, animate the delta) and then keeps animating
  continually per the ORB STATE MAP. Reduce-motion cuts straight to the corner.

  The corner diameter is reported via `onCornerSize` so the transcript can reserve
  a matching circular footprint (`shape-outside`) for text to wrap around.
-->
<script lang="ts">
  import { onMount, tick } from 'svelte';
  import Forge from '$lib/forge/Forge.svelte';
  import type { AgentState } from '$lib/stores/agents';
  import type { OrbState } from '$lib/forge/orbState';

  export let agent: AgentState | null = null;
  export let orb: OrbState;
  export let live = false;
  export let onCornerSize: (px: number) => void = () => {};

  let stageEl: HTMLDivElement;
  let hostEl: HTMLDivElement;
  let w = 320;
  let h = 320;
  let mounted = false;
  let prevLive = false;

  const reduceMotion =
    typeof window !== 'undefined' && window.matchMedia
      ? window.matchMedia('(prefers-reduced-motion: reduce)').matches
      : false;

  // Clamp the orb: ≤300px in chat (floor ~120 on tight tiles); larger when idle.
  $: cornerSize = Math.round(Math.max(120, Math.min(300, Math.min(w, h) * 0.42)));
  $: idleSize = Math.round(Math.max(150, Math.min(320, Math.min(w, h) * 0.55)));
  $: orbSize = live ? cornerSize : idleSize;
  $: onCornerSize(live ? cornerSize + 18 : 0);

  // Absorption FLIP — only on the idle→live transition, never on first mount.
  $: if (mounted) onLiveChange(live);

  function onLiveChange(l: boolean) {
    if (l === prevLive) return;
    const absorbing = prevLive === false && l === true;
    prevLive = l;
    if (absorbing && hostEl && !reduceMotion) {
      void absorb(hostEl.getBoundingClientRect());
    }
  }

  async function absorb(first: DOMRect) {
    await tick(); // let the corner layout + new size settle
    if (!hostEl) return;
    const last = hostEl.getBoundingClientRect();
    if (last.width === 0) return;
    const dx = first.left - last.left;
    const dy = first.top - last.top;
    const scale = first.width / last.width;
    hostEl.style.transformOrigin = 'top left';
    hostEl.style.transition = 'none';
    hostEl.style.transform = `translate(${dx}px, ${dy}px) scale(${scale})`;
    // Next frame: release to the resting corner position in one spring.
    requestAnimationFrame(() => {
      hostEl.style.transition = 'transform 380ms cubic-bezier(0.22, 1, 0.36, 1)';
      hostEl.style.transform = '';
    });
  }

  function clearFlip() {
    if (!hostEl) return;
    hostEl.style.transition = '';
    hostEl.style.transform = '';
    hostEl.style.transformOrigin = '';
  }

  onMount(() => {
    prevLive = live;
    mounted = true;
    const ro = new ResizeObserver((entries) => {
      const r = entries[0]?.contentRect;
      if (r) {
        w = r.width;
        h = r.height;
      }
    });
    if (stageEl) ro.observe(stageEl);
    return () => ro.disconnect();
  });
</script>

<div class="stage" class:idle={!live} bind:this={stageEl}>
  <div class="orb-host" class:corner={live} bind:this={hostEl} on:transitionend={clearFlip}>
    <Forge
      size={orbSize}
      pressure={agent?.pressure ?? 0.2}
      activity={agent?.activity ?? 0.2}
      health={agent?.health ?? 0.85}
      stimulus={agent?.stimulus ?? 0}
      stimulusKind={agent?.stimulusKind ?? 'request'}
      glowColor={orb.glowColor}
      spinSpeed={orb.spinSpeed}
      pulseRate={orb.pulseRate}
      glowIntensity={orb.glowIntensity}
      turbulence={orb.turbulence}
      special={orb.special}
    />
  </div>

  {#if !live}
    <div class="launcher"><slot /></div>
  {/if}
</div>

<style>
  .stage {
    position: relative;
    height: 100%;
    width: 100%;
  }
  /* Live: an absolute overlay over the transcript so the corner orb floats on
     top without intercepting scroll/clicks (pointer-events re-enabled nowhere —
     the orb is non-interactive). */
  .stage:not(.idle) {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }
  .stage.idle {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 1.1rem;
    padding: 1rem;
    overflow-y: auto;
  }
  .orb-host {
    pointer-events: none;
  }
  /* Live: absorbed into the bottom-right corner, just above the composer. */
  .orb-host.corner {
    position: absolute;
    right: 10px;
    bottom: 10px;
    z-index: 4;
  }
  .launcher {
    width: 100%;
    max-width: 22rem;
  }
</style>
