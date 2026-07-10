<!--
  OrbDot — the compact form of the living orb: a single status pip driven
  entirely by an `OrbState` (the same pure description `ForgeStage`'s full
  WebGL orb consumes). Color is `orb.glowColor`; the glow radius tracks
  `glowIntensity`; the motion flourish (`special`) picks the animation. This
  is the orb vocabulary shrunk to fit inline in a `StackCard`, so a card and a
  Forge pane telegraph the exact same state in the exact same colors.
-->
<script lang="ts">
  import type { OrbState } from './orbState';

  export let orb: OrbState;
  /** Screen-reader/hover label — the orb is visual, this is its text alt. */
  export let label = '';

  // Pulse period shrinks as pulseRate rises; a floor keeps it calm at rest.
  $: period = orb.pulseRate > 0 ? Math.max(0.6, 1.6 / orb.pulseRate) : 0;
  $: glow = 3 + orb.glowIntensity * 7;
</script>

<span
  class="orbdot {orb.special}"
  style:--orb={orb.glowColor}
  style:--period={`${period}s`}
  style:--glow={`${glow}px`}
  title={label}
  role="img"
  aria-label={label || 'agent status'}
></span>

<style>
  .orbdot {
    display: inline-block;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--orb);
    box-shadow: 0 0 var(--glow) 0 color-mix(in srgb, var(--orb) 75%, transparent);
    flex: 0 0 auto;
  }
  /* Live phase / queued / throttled all breathe; period encodes the rate. */
  .orbdot.none,
  .orbdot.attentionPulse,
  .orbdot.kryptonite {
    animation: orbpulse var(--period) ease-in-out infinite;
  }
  .orbdot.attentionPulse {
    animation-name: orbattention;
  }
  .orbdot.kryptonite {
    animation-name: orbbloom;
  }
  /* Rate-limited retry — a nervous stutter rather than a smooth pulse. */
  .orbdot.stutter {
    animation: orbstutter 0.9s steps(3, end) infinite;
  }
  /* Rolling back — agitated, keep it moving but distinct from a clean pulse. */
  .orbdot.reverseSpin {
    animation: orbpulse 0.7s ease-in-out infinite;
  }
  /* Failed / cancelled — hard steady rim, no motion (matches hardStop). */
  .orbdot.hardStop {
    animation: none;
  }
  @keyframes orbpulse {
    0%,
    100% {
      opacity: 1;
      box-shadow: 0 0 var(--glow) 0 color-mix(in srgb, var(--orb) 75%, transparent);
    }
    50% {
      opacity: 0.55;
      box-shadow: 0 0 calc(var(--glow) * 0.4) 0 color-mix(in srgb, var(--orb) 40%, transparent);
    }
  }
  @keyframes orbattention {
    0%,
    100% {
      transform: scale(1);
    }
    50% {
      transform: scale(1.35);
    }
  }
  @keyframes orbbloom {
    0% {
      box-shadow: 0 0 calc(var(--glow) * 1.6) 0 color-mix(in srgb, var(--orb) 85%, transparent);
    }
    100% {
      box-shadow: 0 0 var(--glow) 0 color-mix(in srgb, var(--orb) 60%, transparent);
    }
  }
  @keyframes orbstutter {
    0%,
    60%,
    100% {
      opacity: 1;
    }
    30%,
    80% {
      opacity: 0.4;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .orbdot {
      animation: none !important;
    }
  }
</style>
