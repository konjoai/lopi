<!--
  /stacks — the loop-stack composer. Two independent panes, each an ordered
  queue of loop cards with schedule/guardrails/evals popovers, a config
  drawer, connectors, and (for the single running task, once one exists) a
  live output attachment. The existing /loop cockpit is a different surface
  and is left untouched.

  Guardrails/schedule/model/effort/repo round-trip through the real
  `CreateTaskOptions` shape (see `stores/stack.ts::cardToTaskPayload`).
  Backend-1 wired "run stack" for real: each pane's own `stores/stackRun.ts`
  sequencer launches its cards bottom-to-top via `createTask`, and
  pause/resume/drain/bump are a client-side state machine (there's no
  server-side "stack"/"plan" concept — see the sprint's ledger entry).
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import Panel from '$lib/components/ui/Panel.svelte';
  import Dropdown from '$lib/components/ui/Dropdown.svelte';
  import StackPane from '$lib/components/stacks/StackPane.svelte';
  import { panes } from '$lib/stores/stack';
  import { stackDefaults, AUTONOMY_OPTIONS, BRANCH_OPTIONS } from '$lib/stores/stackDefaults';
  import { MODEL_OPTIONS, EFFORT_OPTIONS, type Option } from '$lib/stores/controls';
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
      compose two independent stacks · guardrails/schedule are wired · run stack launches for real
    </p>
  </div>

  <Panel title="Pane defaults" subtitle="model · effort · repo · branch · autonomy every card below starts from">
    <div class="selrow">
      <Dropdown dense label="model" bind:value={$stackDefaults.model} options={MODEL_OPTIONS} />
      <Dropdown dense label="effort" bind:value={$stackDefaults.effort} options={EFFORT_OPTIONS} />
      <Dropdown dense label="repo" bind:value={$stackDefaults.repo} options={repoOptions} />
      <Dropdown dense label="branch" bind:value={$stackDefaults.branch} options={BRANCH_OPTIONS} />
      <div class="autonomy">
        <Dropdown dense label="autonomy" bind:value={$stackDefaults.autonomy} options={AUTONOMY_OPTIONS} />
      </div>
    </div>
  </Panel>

  <div class="panes">
    {#each $panes as pane (pane.key)}
      <StackPane {pane} paneDefaults={$stackDefaults} {repoOptions} />
    {/each}
  </div>
</div>

<style>
  .selrow {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  /* Autonomy is the one violet control in the row — scope the Dropdown's
     accent var locally rather than forking the component. */
  .autonomy {
    --konjo-accent-rgb: 183 155 255;
  }
  .panes {
    display: flex;
    gap: 22px;
    align-items: flex-start;
    justify-content: center;
    flex-wrap: wrap;
  }
</style>
