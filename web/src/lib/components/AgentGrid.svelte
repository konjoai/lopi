<script lang="ts">
  import { onMount } from 'svelte';
  import AgentPane from '$lib/components/AgentPane.svelte';
  import { agents, removeAgent, type AgentState } from '$lib/stores/agents';

  // Always-4 slots grid by default, expands when user clicks + button
  let paneSlots: (string | null)[] = [null, null, null, null];
  let slotCount = 4;
  let draggedSlot: number | null = null;
  let dragOverSlot: number | null = null;

  onMount(() => {
    // Populate initial 4 slots from agents
    const agentIds = Array.from($agents.keys()).slice(0, 4);
    paneSlots = [...agentIds, ...Array(4 - agentIds.length).fill(null)];

    // Listen for add-pane events from header
    const handleAddPane = () => {
      slotCount = Math.min(slotCount + 1, 12); // cap at 12
      paneSlots = paneSlots.length < slotCount ? [...paneSlots, null] : paneSlots;
    };
    // Sidebar → focus a running agent's pane (scroll into view + flash ring).
    const handleFocusAgent = (ev: Event) => {
      const id = (ev as CustomEvent<{ id: string }>).detail?.id;
      if (!id) return;
      const idx = paneSlots.indexOf(id);
      if (idx === -1) return;
      const node = document.querySelector(`[data-slot="${idx}"]`);
      node?.scrollIntoView({ behavior: 'smooth', block: 'center' });
      node?.animate(
        [
          { boxShadow: '0 0 0 0 var(--konjo-ice)' },
          { boxShadow: '0 0 0 4px var(--konjo-ice)' },
          { boxShadow: '0 0 0 0 var(--konjo-ice)' }
        ],
        { duration: 700 }
      );
    };
    // Sidebar → re-open a past task: surface its goal in an empty pane.
    const handleReopen = (ev: Event) => {
      const detail = (ev as CustomEvent<{ goal: string }>).detail;
      if (!detail?.goal) return;
      let slotIdx = paneSlots.indexOf(null);
      if (slotIdx === -1) {
        if (slotCount >= 12) return;
        slotCount += 1;
        paneSlots = [...paneSlots, null];
        slotIdx = paneSlots.length - 1;
      }
      requestAnimationFrame(() => {
        window.dispatchEvent(
          new CustomEvent('lopi:prefill-slot', { detail: { slotIdx, goal: detail.goal } })
        );
      });
    };
    window.addEventListener('lopi:add-pane', handleAddPane);
    window.addEventListener('lopi:focus-agent', handleFocusAgent);
    window.addEventListener('lopi:reopen-task', handleReopen);
    return () => {
      window.removeEventListener('lopi:add-pane', handleAddPane);
      window.removeEventListener('lopi:focus-agent', handleFocusAgent);
      window.removeEventListener('lopi:reopen-task', handleReopen);
    };
  });

  // Subscribe to agents and fill empty slots automatically
  $: {
    const existingIds = new Set(paneSlots.filter((s) => s !== null));
    for (const [id] of $agents) {
      if (!existingIds.has(id)) {
        const emptyIdx = paneSlots.indexOf(null);
        if (emptyIdx !== -1) {
          paneSlots[emptyIdx] = id;
          existingIds.add(id);
        }
      }
    }
  }

  // Compute grid dimensions based on slot count.
  //   1     → 1 × 1            5–6   → 3 cols × 2 rows
  //   2     → 2 cols           7–8   → 4 cols × 2 rows
  //   3     → 3 cols (vertical split as requested)
  //   4     → 2 × 2            9     → 3 × 3
  //                            10–12 → 4 × 3
  function gridDims(n: number): { cols: number; rows: number } {
    if (n <= 1) return { cols: 1, rows: 1 };
    if (n <= 3) return { cols: n, rows: 1 };
    if (n === 4) return { cols: 2, rows: 2 };
    if (n <= 6) return { cols: 3, rows: 2 };
    if (n <= 8) return { cols: 4, rows: 2 };
    if (n === 9) return { cols: 3, rows: 3 };
    if (n <= 12) return { cols: 4, rows: 3 };
    return { cols: 4, rows: Math.ceil(n / 4) };
  }

  $: ({ cols, rows } = gridDims(slotCount));
  // Last pane spans any leftover cells so the screen is always filled.
  $: lastSpan = Math.max(1, cols * rows - slotCount + 1);
  $: gridStyle = `display: grid; grid-template-columns: repeat(${cols}, 1fr); grid-template-rows: repeat(${rows}, 1fr);`;

  // Orb diameter shrinks as the grid fills up — bigger when there's space.
  $: orbSize = (() => {
    if (slotCount <= 1) return 360;
    if (slotCount <= 2) return 280;
    if (slotCount <= 3) return 240;
    if (slotCount <= 4) return 200;
    if (slotCount <= 6) return 170;
    if (slotCount <= 8) return 150;
    if (slotCount <= 9) return 140;
    return 120;
  })();

  function handleDragStart(e: DragEvent, slotIdx: number) {
    draggedSlot = slotIdx;
    e.dataTransfer!.effectAllowed = 'move';
  }

  function handleDragOver(e: DragEvent) {
    e.preventDefault();
    e.dataTransfer!.dropEffect = 'move';
  }

  function handleDragEnter(slotIdx: number) {
    dragOverSlot = slotIdx;
  }

  function handleDragLeave(e: DragEvent) {
    if (e.target === e.currentTarget) {
      dragOverSlot = null;
    }
  }

  function handleDrop(e: DragEvent, dropSlotIdx: number) {
    e.preventDefault();
    if (draggedSlot !== null && draggedSlot !== dropSlotIdx) {
      // Swap the two slots
      [paneSlots[draggedSlot], paneSlots[dropSlotIdx]] = [
        paneSlots[dropSlotIdx],
        paneSlots[draggedSlot]
      ];
      paneSlots = paneSlots; // Trigger reactivity
    }
    draggedSlot = null;
    dragOverSlot = null;
  }

  function handleDragEnd() {
    draggedSlot = null;
    dragOverSlot = null;
  }

  function getAgentForSlot(agentId: string | null): AgentState | null {
    if (!agentId) return null;
    return $agents.get(agentId) ?? null;
  }

  function handleClearSlot(slotIdx: number) {
    const agentId = paneSlots[slotIdx];
    if (agentId) {
      // Live agent — kill the session and leave the slot idle.
      removeAgent(agentId);
      paneSlots[slotIdx] = null;
      paneSlots = paneSlots;
      return;
    }
    // Already idle — splice the slot out so the grid reflows.
    paneSlots = paneSlots.filter((_, i) => i !== slotIdx);
    slotCount = Math.max(0, slotCount - 1);
  }
</script>

<!--
  2×2 (or dynamic) grid layout with draggable panes.
  Always-4 slots by default; expands to 6, 8, 12 when user clicks + button.
  Panes are draggable by their header; drag to swap positions.
-->

<div class="w-full h-full flex flex-col bg-konjo-black overflow-hidden relative">
  <!-- Grid of panes — small gap + small frame so the panes go close to all
       four edges but still have visual breathing room. -->
  <div
    class="flex-1 w-full overflow-hidden"
    style={`${gridStyle} gap: 0.5rem; padding: 0.75rem 0.5rem 0.5rem;`}
    on:dragover={handleDragOver}
  >
    {#each { length: slotCount } as _, slotIdx}
      {@const agentId = paneSlots[slotIdx]}
      {@const agent = getAgentForSlot(agentId)}
      {@const isLast = slotIdx === slotCount - 1}
      <div
        data-slot={slotIdx}
        class="h-full w-full relative overflow-hidden transition-all duration-150"
        class:border-2={dragOverSlot === slotIdx}
        style:border-color={dragOverSlot === slotIdx ? 'var(--konjo-ice)' : 'transparent'}
        style:grid-column={isLast && lastSpan > 1 ? `span ${lastSpan}` : undefined}
        on:dragenter={() => handleDragEnter(slotIdx)}
        on:dragleave={handleDragLeave}
        on:drop={(e) => handleDrop(e, slotIdx)}
        on:dragend={handleDragEnd}
      >
        <div
          class="h-full w-full flex flex-col relative"
          draggable={agent !== null}
          on:dragstart={(e) => handleDragStart(e, slotIdx)}
        >
          <div class="h-full w-full overflow-hidden">
            <AgentPane
              {agent}
              {orbSize}
              slotIndex={slotIdx}
              onClose={() => handleClearSlot(slotIdx)}
            />
          </div>
        </div>
      </div>
    {/each}
  </div>

  <!-- Empty state — only when there are no panes at all (slotCount === 0).
       With idle panes present, each pane shows its own pulse; we don't want
       to layer a screen-wide overlay on top of them. -->
  {#if slotCount === 0}
    <div class="absolute inset-0 flex items-center justify-center pointer-events-none">
      <div class="flex flex-col items-center gap-4">
        <div
          class="rounded-full border-2 border-konjo-ice/25 animate-pulse"
          style:width={`${orbSize}px`}
          style:height={`${orbSize}px`}
          style="box-shadow: 0 0 40px rgba(0,212,255,0.12);"
        ></div>
        <div class="text-center space-y-1.5">
          <div class="font-display text-2xl opacity-40">no agents</div>
          <div class="font-mono text-[11px] uppercase tracking-widest opacity-30">
            click + to add a pane
          </div>
        </div>
      </div>
    </div>
  {/if}
</div>
