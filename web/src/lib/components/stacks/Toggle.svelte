<!-- Toggle — small pill switch shared by the schedule enable row and the
     guardrails gate/until rows. Accent color communicates which popover it
     lives in (ice for schedule, sun for guardrails). -->
<script lang="ts">
  export let on: boolean;
  export let onToggle: () => void;
  export let accent: 'ice' | 'sun' | 'flame' = 'sun';
  /** Blocks the toggle (e.g. `MaxxPopover` disables enabling until the
   *  loop has a goal to actually dispatch) — greys it out and stops
   *  `onToggle` from firing rather than letting the click go through and
   *  surface a server-side rejection instead. */
  export let disabled = false;
</script>

<button
  type="button"
  class="gtog {accent}"
  class:on
  class:disabled
  {disabled}
  on:click={onToggle}
  aria-pressed={on}
>
  <span class="knob"></span>
</button>

<style>
  .gtog {
    width: 30px;
    height: 17px;
    border-radius: 10px;
    background: rgb(var(--k-wash-rgb) / 0.1);
    position: relative;
    cursor: pointer;
    transition: 0.16s;
    flex: 0 0 30px;
    border: none;
    padding: 0;
  }
  .gtog.disabled {
    cursor: not-allowed;
    opacity: 0.4;
  }
  .gtog .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 13px;
    height: 13px;
    border-radius: 50%;
    background: rgb(var(--k-text-primary-rgb) / 0.46);
    transition: 0.16s;
  }
  .gtog.sun.on {
    background: rgb(var(--k-chip-effort-rgb) / 0.28);
  }
  .gtog.sun.on .knob {
    left: 15px;
    background: var(--konjo-sun);
  }
  .gtog.ice.on {
    background: rgb(var(--k-chip-repo-rgb) / 0.28);
  }
  .gtog.ice.on .knob {
    left: 15px;
    background: var(--konjo-ice);
  }
  .gtog.flame.on {
    background: rgb(var(--k-chip-loop-rgb) / 0.28);
  }
  .gtog.flame.on .knob {
    left: 15px;
    background: var(--konjo-flame);
  }
</style>
