<!--
  StatusChip — a compact inline chip for phase transitions, scores, final cost,
  verifier verdicts and rate-limit notices. Color encodes the severity tier.
-->
<script lang="ts">
  import type { ChipTier } from '$lib/stores/transcript';
  export let tier: ChipTier = 'info';
  export let label = '';
</script>

<div class="chip" class:good={tier === 'good'} class:warn={tier === 'warn'} class:bad={tier === 'bad'}>
  <span class="mark"></span>{label}
</div>

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    font-family: var(--font-mono, monospace);
    font-size: 0.65rem;
    letter-spacing: 0.02em;
    padding: 0.15rem 0.5rem;
    margin: 0.25rem 0;
    border-radius: 999px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: rgba(255, 255, 255, 0.03);
    opacity: 0.85;
  }
  .mark {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--konjo-ice);
  }
  .good {
    border-color: color-mix(in srgb, var(--konjo-jade) 40%, transparent);
    color: var(--konjo-jade);
  }
  .good .mark {
    background: var(--konjo-jade);
  }
  .warn {
    border-color: color-mix(in srgb, var(--konjo-flame) 40%, transparent);
    color: var(--konjo-flame);
  }
  .warn .mark {
    background: var(--konjo-flame);
  }
  .bad {
    border-color: color-mix(in srgb, var(--konjo-rose) 45%, transparent);
    color: var(--konjo-rose);
  }
  .bad .mark {
    background: var(--konjo-rose);
  }
</style>
