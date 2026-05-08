<!--
  PhaseWheel — circular SVG indicator showing the agent's current phase.
  Six segments: Boot → Discovery → Planning → Implementation → Testing → Conclusion.
  The active segment glows; completed segments retain a faint trail.
-->
<script lang="ts">
  import { PHASE_COLORS, type Phase } from '$lib/stores/agents';

  export let phase: Phase = 'Boot';
  export let size: number = 120;

  const phases: Phase[] = [
    'Boot',
    'Discovery',
    'Planning',
    'Implementation',
    'Testing',
    'Conclusion'
  ];

  $: activeIndex = phases.indexOf(phase);

  // Geometry
  $: radius = size / 2 - 6;
  $: cx = size / 2;
  $: cy = size / 2;
  $: segmentAngle = (Math.PI * 2) / phases.length;

  function arcPath(i: number, r: number): string {
    const startAngle = i * segmentAngle - Math.PI / 2;
    const endAngle = startAngle + segmentAngle - 0.06; // small gap between segments
    const x1 = cx + r * Math.cos(startAngle);
    const y1 = cy + r * Math.sin(startAngle);
    const x2 = cx + r * Math.cos(endAngle);
    const y2 = cy + r * Math.sin(endAngle);
    return `M ${x1} ${y1} A ${r} ${r} 0 0 1 ${x2} ${y2}`;
  }
</script>

<div class="relative inline-block" style="width: {size}px; height: {size}px;">
  <svg viewBox="0 0 {size} {size}" width={size} height={size} class="block">
    {#each phases as p, i}
      {@const color = PHASE_COLORS[p]}
      {@const isActive = i === activeIndex}
      {@const isPast = i < activeIndex}
      <path
        d={arcPath(i, radius)}
        stroke={color}
        stroke-width="3"
        fill="none"
        stroke-linecap="round"
        opacity={isActive ? 1 : isPast ? 0.5 : 0.12}
        style:filter={isActive ? `drop-shadow(0 0 10px ${color})` : 'none'}
        class="transition-all duration-700"
      />
    {/each}
  </svg>

  <!-- Center label -->
  <div
    class="absolute inset-0 flex flex-col items-center justify-center pointer-events-none"
  >
    <span
      class="font-mono text-[10px] uppercase tracking-widest opacity-60"
    >
      phase
    </span>
    <span
      class="font-sans font-semibold text-sm transition-colors duration-700"
      style:color={PHASE_COLORS[phase]}
    >
      {phase}
    </span>
  </div>
</div>
