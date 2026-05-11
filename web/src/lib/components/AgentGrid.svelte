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
    window.addEventListener('lopi:add-pane', handleAddPane);
    return () => window.removeEventListener('lopi:add-pane', handleAddPane);
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

  // Compute grid dimensions based on slot count (equal-sized panes, fill viewport)
  $: cols =
    slotCount === 1
      ? 1
      : slotCount === 2
        ? 2
        : slotCount <= 4
          ? 2
          : slotCount <= 6
            ? 3
            : 4;
  $: rows = Math.ceil(slotCount / cols);
  $: gridStyle = `display: grid; grid-template-columns: repeat(${cols}, 1fr); grid-template-rows: repeat(${rows}, 1fr);`;

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
    if (agentId) removeAgent(agentId);
    paneSlots[slotIdx] = null;
    paneSlots = paneSlots;
  }
</script>

<!--
  2×2 (or dynamic) grid layout with draggable panes.
  Always-4 slots by default; expands to 6, 8, 12 when user clicks + button.
  Panes are draggable by their header; drag to swap positions.
-->

<div class="w-full h-full flex flex-col bg-konjo-black overflow-hidden">
  <!-- Grid of panes (equal-sized, fills 100%) -->
  <div
    class="flex-1 w-full overflow-hidden"
    style={`${gridStyle} gap: 0.75rem; padding: 0.75rem;`}
    on:dragover={handleDragOver}
  >
    {#each { length: slotCount } as _, slotIdx}
      {@const agentId = paneSlots[slotIdx]}
      {@const agent = getAgentForSlot(agentId)}
      <div
        class="h-full w-full relative overflow-hidden transition-all duration-150"
        class:border-2={dragOverSlot === slotIdx}
        style:border-color={dragOverSlot === slotIdx ? 'var(--konjo-ice)' : 'transparent'}
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
          <!-- AgentPane or empty slot -->
          <div class="h-full w-full overflow-hidden">
            <AgentPane {agent} slotIndex={slotIdx} onClose={agent ? () => handleClearSlot(slotIdx) : null} />
          </div>

        </div>
      </div>
    {/each}
  </div>

  <!-- Empty state (only if all slots are empty) -->
  {#if paneSlots.every((s) => s === null)}
    <div class="absolute inset-0 flex items-center justify-center pointer-events-none">
      <div class="text-center space-y-2">
        <div class="font-display text-xl opacity-30">no agents</div>
        <div class="font-mono text-[10px] uppercase tracking-widest opacity-20">type a goal in any pane to start</div>
      </div>
    </div>
  {/if}
</div>
