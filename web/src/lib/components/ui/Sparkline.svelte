<script lang="ts">
  /**
   * Minimal SVG sparkline — no dependencies. Draws a polyline over the
   * series with a soft gradient fill and a dot on the latest point.
   */
  export let values: number[] = [];
  export let width = 280;
  export let height = 48;
  export let color = 'var(--konjo-accent)';
  /** Fixed value domain; pass null to auto-fit the series. */
  export let min: number | null = 0;
  export let max: number | null = 1;

  const PAD = 4;

  $: lo = min ?? Math.min(...values);
  $: hi = max ?? Math.max(...values);
  $: span = hi - lo || 1;
  $: pts = values.map((v, i) => {
    const x =
      values.length === 1
        ? width / 2
        : PAD + (i / (values.length - 1)) * (width - PAD * 2);
    const y = height - PAD - ((v - lo) / span) * (height - PAD * 2);
    return [x, y] as const;
  });
  $: line = pts.map(([x, y]) => `${x},${y}`).join(' ');
  $: area =
    pts.length > 1
      ? `${pts[0][0]},${height - PAD} ${line} ${pts[pts.length - 1][0]},${height - PAD}`
      : '';
  $: last = pts.length > 0 ? pts[pts.length - 1] : null;
</script>

<svg {width} {height} viewBox="0 0 {width} {height}" class="block" role="img" aria-label="trend sparkline">
  {#if area}
    <polygon points={area} fill={color} opacity="0.08" />
  {/if}
  {#if pts.length > 1}
    <polyline
      points={line}
      fill="none"
      stroke={color}
      stroke-width="1.5"
      stroke-linejoin="round"
      stroke-linecap="round"
    />
  {/if}
  {#if last}
    <circle cx={last[0]} cy={last[1]} r="2.5" fill={color}>
      <animate attributeName="opacity" values="1;0.4;1" dur="2s" repeatCount="indefinite" />
    </circle>
  {/if}
</svg>
