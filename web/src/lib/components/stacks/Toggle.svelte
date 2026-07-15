<!-- Toggle — small pill switch shared by the schedule enable row and the
     guardrails gate/until rows. Accent color communicates which popover it
     lives in (ice for schedule, sun for guardrails). -->
<script lang="ts">
  export let on: boolean;
  export let onToggle: () => void;
  export let accent: 'ice' | 'sun' | 'flame' = 'sun';
</script>

<button type="button" class="gtog {accent}" class:on on:click={onToggle} aria-pressed={on}>
  <span class="knob"></span>
</button>

<style>
  .gtog {
    width: 30px;
    height: 17px;
    border-radius: 10px;
    background: rgba(255, 255, 255, 0.1);
    position: relative;
    cursor: pointer;
    transition: 0.16s;
    flex: 0 0 30px;
    border: none;
    padding: 0;
  }
  .gtog .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 13px;
    height: 13px;
    border-radius: 50%;
    background: rgba(245, 245, 245, 0.46);
    transition: 0.16s;
  }
  .gtog.sun.on {
    background: rgba(255, 204, 0, 0.28);
  }
  .gtog.sun.on .knob {
    left: 15px;
    background: var(--konjo-sun);
  }
  .gtog.ice.on {
    background: rgba(0, 212, 255, 0.28);
  }
  .gtog.ice.on .knob {
    left: 15px;
    background: var(--konjo-ice);
  }
  .gtog.flame.on {
    background: rgba(255, 149, 0, 0.28);
  }
  .gtog.flame.on .knob {
    left: 15px;
    background: var(--konjo-flame);
  }
</style>
