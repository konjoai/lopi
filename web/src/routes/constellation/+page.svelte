<!--
  /constellation — full-canvas orbital view of every agent.
  Click any body → focuses it and returns to / for the deep-dive Forge view.
-->
<script lang="ts">
  import Constellation from '$lib/components/Constellation.svelte';
  import { goto } from '$app/navigation';
  import { agents, stats } from '$lib/stores/agents';

  function handleSelect(id: string) {
    if (id) goto('/');
  }
</script>

<!-- Inset top stats — shown over the canvas -->
<div
  class="absolute top-16 inset-x-0 z-20 px-6 flex items-baseline justify-between pointer-events-none"
>
  <div>
    <div class="font-display text-3xl tracking-tight">Constellation</div>
    <div class="font-mono text-[10px] uppercase tracking-widest opacity-50 mt-1">
      every agent, in flight
    </div>
  </div>
  <div class="grid grid-cols-3 gap-6 pointer-events-auto">
    {#each [['running', $stats.running], ['queued', $stats.queued], ['done', $stats.completed]] as [label, val]}
      <div class="text-right">
        <div class="font-mono text-[10px] uppercase tracking-widest opacity-50">{label}</div>
        <div class="font-display text-2xl tabular-nums">{val}</div>
      </div>
    {/each}
  </div>
</div>

<Constellation onSelect={handleSelect} />

<!-- Footer hint -->
<div
  class="fixed bottom-4 inset-x-0 text-center font-mono text-[10px] uppercase tracking-widest opacity-40 pointer-events-none"
>
  hover any body to inspect · click to focus the forge
</div>
