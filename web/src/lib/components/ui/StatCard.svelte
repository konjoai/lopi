<script lang="ts">
  /** Big-number stat tile used across the Debug + Tasks tabs. */
  export let label: string;
  export let value: string | number;
  export let color: string = 'var(--konjo-accent)';
</script>

<div
  class="rounded-lg border border-white/5 bg-konjo-deep/60 px-4 py-3 flex flex-col gap-1 transition-colors hover:border-white/15"
>
  <span class="font-mono text-[9px] uppercase tracking-widest opacity-40">{label}</span>
  <span class="font-display text-2xl font-bold tabular-nums leading-none" style:color>
    {value}
  </span>
</div>
