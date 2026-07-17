<!--
  /stacks — Loop Stacks, the single primary working surface (Unify-2 §3).

  The one collapsed pane primitive lives here: `StackPane`s laid out in the
  auto-tiling, drag-resizable `TileGrid` (the one capability worth keeping from
  the old Forge). A pane defaults to a *bare* box — top composer, a single loop
  card + its orb, no connector, no control dock — so it reads exactly like a
  pre-Unify Forge pane. Add a second loop and the connector + purple stack
  control dock appear, and it behaves like Stacks always has: schedule /
  guardrails / evals / config for the whole chain, plus the real run-stack
  action (each pane's `stores/stackRun.ts` sequencer launches its cards
  bottom-to-top via `createTask`).

  The topbar `+` adds a fresh pane; a pane's header `✕` closes it (the last
  pane can't be closed). This replaces the retired Forge component tree
  (`AgentGrid` / `AgentPane` / `SessionSidebar`) — every launch still flows
  through the one unified `createTask`, and every live agent is keyed back to
  its card by `taskId` for the orb.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import TileGrid from '$lib/components/TileGrid.svelte';
  import StackPane from '$lib/components/stacks/StackPane.svelte';
  import { panes, addStackPane, deleteStackFromPanes } from '$lib/stores/stack';
  import type { Option } from '$lib/stores/controls';
  import { AUTO_OPTION, repoOptions as buildRepoOptions } from '$lib/stores/repoMenu';
  import { listRepos } from '$lib/api';

  let repoOptions: Option[] = [AUTO_OPTION];

  onMount(() => {
    (async () => {
      try {
        const { repos } = await listRepos();
        // Labels, grouping and order are one pure rule shared with the macOS
        // port and pinned by a golden fixture — see `stores/repoMenu.ts`.
        if (repos.length) repoOptions = buildRepoOptions(repos);
      } catch {
        // Repo listing is best-effort chrome — the composer works with the
        // "auto" default if /api/repos is unreachable (e.g. static preview).
      }
    })();

    // The topbar "+" (in +layout.svelte) dispatches this on Loop Stacks.
    const onAdd = () => addStackPane();
    window.addEventListener('lopi:add-pane', onAdd);
    return () => window.removeEventListener('lopi:add-pane', onAdd);
  });
</script>

<div class="loopstack">
  <TileGrid count={$panes.length} let:index>
    {#if $panes[index]}
      <StackPane
        pane={$panes[index]}
        {index}
        {repoOptions}
        onClose={$panes.length > 1 ? () => deleteStackFromPanes($panes[index].key) : null}
      />
    {/if}
  </TileGrid>
</div>

<style>
  .loopstack {
    width: 100%;
    height: 100%;
  }
</style>
