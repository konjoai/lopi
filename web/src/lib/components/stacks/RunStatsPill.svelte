<!--
  RunStatsPill — a small live status readout (elapsed time, tokens, cost) for
  a card actively running, matching `.iterpill`'s pill language in
  StackCard.svelte. Fed with plain numbers rather than an `AgentState` so it
  stays independently testable and has no store dependency of its own — the
  caller (StackCard.svelte) reads `$agents.get(card.taskId)` and passes the
  three numbers down.
-->
<script lang="ts">
  import { ICONS } from './icons';
  import { formatElapsed, formatTokens, formatCost } from './runStats';

  export let elapsedMs: number;
  export let tokens: number;
  export let costUsd: number;

  $: elapsedLabel = formatElapsed(elapsedMs);
  $: tokensLabel = formatTokens(tokens);
  $: costLabel = formatCost(costUsd);
</script>

<span
  class="statpill"
  title={`${elapsedLabel} elapsed · ${tokens.toLocaleString()} tokens · ${costLabel} spent`}
>
  {@html ICONS.spinner}
  <span class="v">{elapsedLabel}</span>
  <span class="sep">·</span>
  <span class="v">{tokensLabel} tok</span>
  <span class="sep">·</span>
  <span class="v cost">{costLabel}</span>
</span>

<style>
  .statpill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 29px;
    padding: 0 9px;
    border: 1px solid rgba(0, 212, 255, 0.35);
    background: rgba(0, 212, 255, 0.06);
    border-radius: 6px;
    font-size: 10.5px;
    font-family: var(--font-mono, monospace);
    color: var(--konjo-ice);
    font-weight: 600;
    white-space: nowrap;
  }
  .statpill :global(svg) {
    width: 12px;
    height: 12px;
    flex: 0 0 auto;
  }
  .statpill :global(svg.spin) {
    animation: spin 1.1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .sep {
    color: rgba(0, 212, 255, 0.3);
  }
  .v.cost {
    color: var(--konjo-jade);
  }
  @media (prefers-reduced-motion: reduce) {
    .statpill :global(svg.spin) {
      animation: none;
    }
  }
</style>
