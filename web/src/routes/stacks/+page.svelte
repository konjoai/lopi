<!--
  /stacks — the loop-stack composer. Independent panes (Stack-1 added
  duplicate/reorder/delete, so there can be more than the original two),
  each an ordered queue of loop cards with schedule/guardrails/evals
  popovers, a config drawer, connectors, a live output attachment for the
  running task, and — pinned at its base — the purple stack control area
  (`StackControlDock.svelte`): loop/schedule/guardrails/evals/config for the
  whole chain, plus the real run-stack action. The existing /loop cockpit is
  a different surface and is left untouched.

  Guardrails/schedule/model/effort/repo round-trip through the real
  `CreateTaskOptions` shape (see `stores/stack.ts::cardToTaskPayload`).
  Backend-1 wired "run stack" for real: each pane's own `stores/stackRun.ts`
  sequencer launches its cards bottom-to-top via `createTask`, and
  pause/resume/drain/bump are a client-side state machine (there's no
  server-side "stack"/"plan" concept — see the sprint's ledger entry).
  Stack-1 moved "pane defaults" from a single app-wide selector row here
  into each pane's own `config.defaults` (its control dock's config
  popover) — a loop's own config still overrides its stack's default, which
  falls back to the app-wide baseline (`loop ?? stack.default ?? DEF`).
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import StackPane from '$lib/components/stacks/StackPane.svelte';
  import { panes } from '$lib/stores/stack';
  import type { Option } from '$lib/stores/controls';
  import { listRepos } from '$lib/api';

  let repoOptions: Option[] = [{ value: '', label: 'auto' }];

  onMount(async () => {
    try {
      const { repos } = await listRepos();
      if (repos.length) {
        repoOptions = [{ value: '', label: 'auto' }, ...repos.map((r) => ({ value: r, label: r }))];
      }
    } catch {
      // Repo listing is best-effort chrome — the composer works with the
      // "auto" default if /api/repos is unreachable (e.g. static preview).
    }
  });
</script>

<div class="max-w-[1400px] mx-auto px-4 py-8 space-y-6">
  <div>
    <h1 class="font-display text-2xl">Loop Stack</h1>
    <p class="font-mono text-[11px] uppercase tracking-widest opacity-50 mt-1">
      compose independent stacks · each carries its own defaults/schedule/guardrails/evals · run stack launches for real
    </p>
  </div>

  <div class="panes">
    {#each $panes as pane, i (pane.key)}
      <StackPane {pane} index={i} {repoOptions} />
    {/each}
  </div>
</div>

<style>
  .panes {
    display: flex;
    gap: 22px;
    align-items: flex-start;
    justify-content: center;
    flex-wrap: wrap;
  }
</style>
