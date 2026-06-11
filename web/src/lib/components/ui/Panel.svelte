<script lang="ts">
  /** Section card — shared chrome for every data tab. */
  export let title: string;
  export let subtitle: string = '';
</script>

<section
  class="rounded-lg border border-white/5 bg-konjo-deep/60 backdrop-blur-sm overflow-hidden animate-panel-in"
>
  <header class="px-4 py-3 border-b border-white/5 flex items-center justify-between gap-3">
    <div class="min-w-0">
      <h2 class="font-display text-sm font-bold tracking-tight">{title}</h2>
      {#if subtitle}
        <p class="font-mono text-[10px] uppercase tracking-widest opacity-40 mt-0.5">{subtitle}</p>
      {/if}
    </div>
    <div class="flex items-center gap-2 flex-shrink-0">
      <slot name="actions" />
    </div>
  </header>
  <div class="p-4">
    <slot />
  </div>
</section>

<style>
  .animate-panel-in {
    animation: panel-in 0.35s cubic-bezier(0.16, 1, 0.3, 1) both;
  }
  @keyframes panel-in {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
</style>
