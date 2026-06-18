<script lang="ts">
  /**
   * Auto-tiling, drag-resizable pane grid.
   *
   * The layout is driven purely by `count`: 1 = full, 2 = side-by-side halves,
   * 3 = thirds, 4 = quarters (2×2), then 3-wide as it grows. Adding or removing
   * a pane re-tiles instantly. Between every pair of columns/rows sits a gutter
   * the operator can drag to bias the split; the fractions reset to even
   * whenever the tiling shape changes.
   *
   * Children are provided through a single scoped slot rendered once per cell:
   *   <TileGrid {count} let:index>…uses index…</TileGrid>
   */
  import { tileDims } from '$lib/stores/layout-core';
  import { flip } from 'svelte/animate';
  import { scale } from 'svelte/transition';
  import { cubicOut, backOut } from 'svelte/easing';

  export let count: number;

  // Keyed cell list so add/remove animates the *changed* tile and FLIP glides
  // the survivors to their new tracks. Resizing a gutter doesn't touch this
  // list, so the spring never fights a live drag.
  $: cells = Array.from({ length: count }, (_, i) => i);

  let W = 0;
  let H = 0;
  let colFr: number[] = [];
  let rowFr: number[] = [];

  $: [cols, rows] = tileDims(count);
  // Reset fractions to even whenever the grid shape changes.
  $: if (colFr.length !== cols) colFr = Array.from({ length: cols }, () => 1);
  $: if (rowFr.length !== rows) rowFr = Array.from({ length: rows }, () => 1);

  $: colTemplate = colFr.map((f) => `${f}fr`).join(' ');
  $: rowTemplate = rowFr.map((f) => `${f}fr`).join(' ');

  const MIN_FRAC = 0.18; // a pane can shrink to ~18% of its track group

  function boundaries(fr: number[], extent: number): number[] {
    const total = fr.reduce((a, b) => a + b, 0) || 1;
    const out: number[] = [];
    let acc = 0;
    for (let i = 0; i < fr.length - 1; i++) {
      acc += fr[i];
      out.push((acc / total) * extent);
    }
    return out;
  }

  $: colBounds = boundaries(colFr, W);
  $: rowBounds = boundaries(rowFr, H);

  type Drag = { axis: 'col' | 'row'; index: number; start: number; a: number; b: number };
  let drag: Drag | null = null;

  function startDrag(axis: 'col' | 'row', index: number, e: PointerEvent) {
    const fr = axis === 'col' ? colFr : rowFr;
    drag = { axis, index, start: axis === 'col' ? e.clientX : e.clientY, a: fr[index], b: fr[index + 1] };
    (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
  }

  function onMove(e: PointerEvent) {
    if (!drag) return;
    const extent = drag.axis === 'col' ? W : H;
    if (extent <= 0) return;
    const fr = drag.axis === 'col' ? colFr : rowFr;
    const total = fr.reduce((a, b) => a + b, 0) || 1;
    const pos = drag.axis === 'col' ? e.clientX : e.clientY;
    const deltaFr = ((pos - drag.start) / extent) * total;
    const min = MIN_FRAC * total;
    let a = drag.a + deltaFr;
    let b = drag.b - deltaFr;
    if (a < min) {
      b -= min - a;
      a = min;
    }
    if (b < min) {
      a -= min - b;
      b = min;
    }
    const next = [...fr];
    next[drag.index] = a;
    next[drag.index + 1] = b;
    if (drag.axis === 'col') colFr = next;
    else rowFr = next;
  }

  function endDrag() {
    drag = null;
  }
</script>

<svelte:window on:pointermove={onMove} on:pointerup={endDrag} />

<div
  class="tilegrid"
  class:dragging={drag !== null}
  bind:clientWidth={W}
  bind:clientHeight={H}
  style:grid-template-columns={colTemplate}
  style:grid-template-rows={rowTemplate}
>
  {#each cells as index (index)}
    <div
      class="cell"
      animate:flip={{ duration: 420, easing: cubicOut }}
      in:scale|local={{ duration: 320, start: 0.86, opacity: 0, easing: backOut }}
      out:scale|local={{ duration: 200, start: 0.92, opacity: 0, easing: cubicOut }}
    >
      <slot {index} />
    </div>
  {/each}

  <!-- Column gutters (vertical) -->
  {#each colBounds as x, i}
    <button
      type="button"
      class="gutter v"
      class:active={drag?.axis === 'col' && drag.index === i}
      style:left={`${x}px`}
      on:pointerdown={(e) => startDrag('col', i, e)}
      aria-label="Resize columns"
    ></button>
  {/each}
  <!-- Row gutters (horizontal) -->
  {#each rowBounds as y, i}
    <button
      type="button"
      class="gutter h"
      class:active={drag?.axis === 'row' && drag.index === i}
      style:top={`${y}px`}
      on:pointerdown={(e) => startDrag('row', i, e)}
      aria-label="Resize rows"
    ></button>
  {/each}
</div>

<style>
  .tilegrid {
    position: relative;
    width: 100%;
    height: 100%;
    display: grid;
    gap: 10px;
    padding: 10px;
    box-sizing: border-box;
  }
  .cell {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .gutter {
    position: absolute;
    z-index: 20;
    border: none;
    background: transparent;
    padding: 0;
    /* Glide to the new boundary when the grid re-flows; snap while dragging. */
    transition:
      background 0.12s,
      left 0.42s cubic-bezier(0.16, 1, 0.3, 1),
      top 0.42s cubic-bezier(0.16, 1, 0.3, 1);
  }
  .dragging .gutter {
    transition: background 0.12s;
  }
  .gutter::after {
    content: '';
    position: absolute;
    border-radius: 2px;
    background: transparent;
    transition: background 0.12s;
  }
  .gutter.v {
    top: 0;
    bottom: 0;
    width: 12px;
    transform: translateX(-6px);
    cursor: col-resize;
  }
  .gutter.v::after {
    top: 0;
    bottom: 0;
    left: 5px;
    width: 2px;
  }
  .gutter.h {
    left: 0;
    right: 0;
    height: 12px;
    transform: translateY(-6px);
    cursor: row-resize;
  }
  .gutter.h::after {
    left: 0;
    right: 0;
    top: 5px;
    height: 2px;
  }
  .gutter:hover::after,
  .gutter.active::after {
    background: rgb(var(--konjo-accent-rgb) / 0.6);
    box-shadow: 0 0 10px rgb(var(--konjo-accent-rgb) / 0.4);
  }
</style>
