<!--
  ThoughtStream — animated display of the agent's current planning text.
  Letterforms fade in one-by-one as if emerging from cognition.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  export let thought: string = '';

  let displayed = '';
  let cursor = 0;
  let raf: number | null = null;
  let lastThought = '';

  $: if (thought !== lastThought) {
    lastThought = thought;
    cursor = 0;
    displayed = '';
    if (raf !== null) cancelAnimationFrame(raf);
    typewriter();
  }

  function typewriter() {
    if (cursor < thought.length) {
      displayed = thought.slice(0, cursor + 1);
      cursor += 1;
      raf = requestAnimationFrame(() => setTimeout(typewriter, 18));
    }
  }

  onMount(() => {
    typewriter();
    return () => {
      if (raf !== null) cancelAnimationFrame(raf);
    };
  });
</script>

<div class="font-mono text-sm leading-relaxed text-konjo-paper/80 max-w-2xl">
  <span class="text-konjo-ice/60 mr-2">⟂</span>{displayed}<span
    class="inline-block w-2 h-4 ml-0.5 bg-konjo-ice/60 align-middle animate-flicker"
  ></span>
</div>
