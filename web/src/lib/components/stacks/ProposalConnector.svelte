<!--
  ProposalConnector — the short violet gap between a card and the ghost card
  it spawned (Ghost Card in the Stack handoff, direction 1a). Structurally
  mirrors StackConnector.svelte's centered-line + pill, but purely
  informational: no "add between" affordance, since a proposal already
  occupies this gap.
-->
<script lang="ts">
  export let loopNumber: number;
</script>

<div class="propconn" aria-hidden="true">
  <span class="pline"></span>
  <span class="ppill">proposed after loop {loopNumber}</span>
</div>

<style>
  .propconn {
    position: relative;
    height: 40px;
    margin: 2px 0;
  }
  .pline {
    position: absolute;
    left: 50%;
    top: 0;
    bottom: 0;
    border-left: 2px dashed rgba(157, 92, 255, 0.5);
    transform: translateX(-1px);
  }
  .ppill {
    position: absolute;
    left: 50%;
    top: 50%;
    transform: translate(-50%, -50%);
    display: inline-flex;
    align-items: center;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    font-size: 9.5px;
    color: var(--stack-violet-deep, #9d5cff);
    border: 1px solid rgba(157, 92, 255, 0.45);
    border-radius: 20px;
    padding: 4px 12px;
    background: var(--konjo-black, #0b0e10);
    white-space: nowrap;
    z-index: 2;
  }
</style>
