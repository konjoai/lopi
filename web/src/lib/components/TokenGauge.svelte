<!--
  TokenGauge — vertical pressure bar.
  Fills cool blue at low pressure, hot orange at high.
  The eviction threshold (75%) is shown as a bright reference line.
  Pulses when pressure exceeds the threshold.
-->
<script lang="ts">
  export let pressure: number = 0.4; // 0..1
  export let height: number = 240;
  export let evictionThreshold: number = 0.75;

  // Smooth color shift: cool → warm
  $: hue = 200 - pressure * 180; // 200 (ice) → 20 (ember)
  $: barColor = `hsl(${hue}, 100%, 50%)`;
  $: alarming = pressure > evictionThreshold;
</script>

<div class="flex flex-col items-center gap-2">
  <span class="font-mono text-[10px] uppercase tracking-widest opacity-50">context</span>

  <div
    class="relative w-2 rounded-full overflow-hidden bg-white/5 ring-1 ring-white/5"
    style="height: {height}px;"
  >
    <!-- Eviction threshold line -->
    <div
      class="absolute left-0 right-0 h-px bg-konjo-flame/60 z-10"
      style="bottom: {evictionThreshold * 100}%;"
    ></div>

    <!-- Filled portion -->
    <div
      class="absolute left-0 right-0 bottom-0 transition-all duration-500 ease-out"
      class:animate-pulse={alarming}
      style:height={`${pressure * 100}%`}
      style:background={`linear-gradient(to top, var(--konjo-ice) 0%, ${barColor} 100%)`}
      style:box-shadow={alarming ? `0 0 10px ${barColor}` : 'none'}
    ></div>
  </div>

  <span class="font-mono text-xs tabular-nums" style:color={alarming ? 'var(--konjo-flame)' : 'inherit'}>
    {Math.round(pressure * 100)}%
  </span>
</div>
