<!--
  CostCounter — accumulated cost in USD with a smooth count-up animation.
-->
<script lang="ts">
  export let cost: number = 0;
  export let cap: number = 1.0; // circuit-breaker cap

  let displayed = 0;
  $: target = cost;
  $: ratio = Math.min(1, cost / cap);
  $: warmth = ratio; // 0..1 — drives color shift

  // Smooth interpolation
  let raf: number | null = null;
  function animate() {
    displayed += (target - displayed) * 0.1;
    if (Math.abs(target - displayed) > 0.0001) {
      raf = requestAnimationFrame(animate);
    } else {
      raf = null;
    }
  }
  $: if (Math.abs(target - displayed) > 0.0001 && !raf) {
    raf = requestAnimationFrame(animate);
  }

  $: color =
    warmth < 0.5
      ? 'var(--konjo-paper)'
      : warmth < 0.8
        ? 'var(--konjo-sun)'
        : 'var(--konjo-rose)';
</script>

<div class="flex flex-col items-end">
  <span class="font-mono text-[10px] uppercase tracking-widest opacity-50">spend</span>
  <span class="font-mono text-2xl font-semibold tabular-nums transition-colors" style:color>
    ${displayed.toFixed(4)}
  </span>
</div>
