<!--
  LopiWordmark — the "l[loop-icon]pi" brand lockup: plain letters either side
  of the loop-stack mark standing in for the "o". Letters render in
  `currentColor` so parent `:hover`/theme rules (e.g. the topbar link) still
  drive their color; only the mark itself is pinned to the flame accent.
-->
<script lang="ts">
  import { ICONS } from './stacks/icons';

  export let size: number = 18;
</script>

<span class="wordmark" role="img" aria-label="lopi" style:font-size={`${size}px`}>
  <span aria-hidden="true">l</span>
  <span
    class="mark"
    aria-hidden="true"
    style:width={`${size * 0.92}px`}
    style:height={`${size * 0.92}px`}
  >{@html ICONS.loop}</span>
  <span aria-hidden="true">pi</span>
</span>

<style>
  .wordmark {
    display: inline-flex;
    align-items: center;
    gap: 0.03em;
    font: inherit;
    line-height: 1;
    color: inherit;
  }
  .mark {
    display: inline-flex;
    flex: 0 0 auto;
    color: var(--konjo-flame, #ff9500);
  }
  .mark :global(svg) {
    width: 100%;
    height: 100%;
  }
</style>
