<script lang="ts">
  /**
   * The Forge — sessions sidebar + a resizable, auto-tiling grid of agent panes.
   *
   * Panes are driven entirely by the persisted `paneSlots` layout, so closing a
   * pane parks its session in the sidebar instead of deleting it, and a reload
   * restores exactly the grid the operator left behind. Default is four panes.
   */
  import { onMount } from 'svelte';
  import AgentPane from '$lib/components/AgentPane.svelte';
  import SessionSidebar from '$lib/components/SessionSidebar.svelte';
  import TileGrid from '$lib/components/TileGrid.svelte';
  import { agents, type AgentState } from '$lib/stores/agents';
  import { paneSlots, closePane, addPane, removePane, swapPanes, mountInPane } from '$lib/stores/layout';

  let dragSource: number | null = null;
  let dragOver: number | null = null;

  onMount(() => {
    const onAdd = () => addPane();
    const onRemove = () => removePane();
    window.addEventListener('lopi:add-pane', onAdd);
    window.addEventListener('lopi:remove-pane', onRemove);
    return () => {
      window.removeEventListener('lopi:add-pane', onAdd);
      window.removeEventListener('lopi:remove-pane', onRemove);
    };
  });

  // Reactive slot→agent resolution. This MUST be a reactive statement that
  // names `$paneSlots` and `$agents` directly — a function called in markup
  // (`agent={agentFor(index)}`) is only re-evaluated when the *identifiers in
  // the expression* change, and Svelte never looks inside the function body.
  // With a plain helper the grid renders once at mount (agents still empty,
  // mock data arrives ~1.5s later) and then freezes on the idle state forever.
  $: paneAgents = $paneSlots.map((id): AgentState | null =>
    id ? ($agents.get(id) ?? null) : null
  );

  function onDragStart(e: DragEvent, index: number) {
    dragSource = index;
    e.dataTransfer!.effectAllowed = 'move';
  }
  function onDrop(e: DragEvent, index: number) {
    e.preventDefault();
    // A session dragged in from the sidebar mounts into this exact pane;
    // an internal drag reorders (swaps) two panes.
    const sessionId = e.dataTransfer?.getData('application/x-lopi-session');
    if (sessionId) {
      mountInPane(sessionId, index);
    } else if (dragSource !== null && dragSource !== index) {
      swapPanes(dragSource, index);
    }
    dragSource = null;
    dragOver = null;
  }
</script>

<div class="forge">
  <SessionSidebar />

  <div class="grid-wrap">
    <TileGrid count={$paneSlots.length} let:index>
      <div
        class="pane-host"
        class:dragover={dragOver === index}
        role="group"
        on:dragover|preventDefault={() => (dragOver = index)}
        on:dragleave={() => dragOver === index && (dragOver = null)}
        on:drop={(e) => onDrop(e, index)}
      >
        <div
          class="drag-handle"
          draggable={paneAgents[index] !== null}
          role="button"
          tabindex="-1"
          on:dragstart={(e) => onDragStart(e, index)}
          title="Drag to reorder"
        ></div>
        <AgentPane agent={paneAgents[index]} onClose={() => closePane(index)} />
      </div>
    </TileGrid>
  </div>
</div>

<style>
  .forge {
    width: 100%;
    height: 100%;
    display: flex;
    background: var(--konjo-black, #0a0a0a);
    overflow: hidden;
  }
  .grid-wrap {
    flex: 1;
    min-width: 0;
    position: relative;
  }
  .pane-host {
    position: relative;
    width: 100%;
    height: 100%;
    border-radius: 10px;
    transition: box-shadow 0.15s;
  }
  .pane-host.dragover {
    box-shadow: 0 0 0 2px var(--konjo-accent);
  }
  .drag-handle {
    position: absolute;
    top: 0;
    left: 0;
    right: 64px; /* leave the pane's own right-rail controls clickable */
    height: 38px;
    z-index: 10;
    cursor: grab;
  }
  .drag-handle:active {
    cursor: grabbing;
  }
</style>
