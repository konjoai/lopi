<script lang="ts">
  /** Graceful empty / offline / error state for the data tabs. */
  export let title: string = 'nothing here yet';
  export let detail: string = '';
  export let error: boolean = false;
</script>

<div class="py-10 text-center select-none">
  <div
    class="font-display text-lg mb-1"
    class:opacity-60={!error}
    style:color={error ? 'var(--konjo-rose)' : 'inherit'}
  >
    {title}
  </div>
  {#if detail}
    <div class="font-mono text-[10px] uppercase tracking-widest opacity-40">{detail}</div>
  {/if}
</div>
