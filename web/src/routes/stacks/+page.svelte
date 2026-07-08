<!--
  /stacks — the loop-stack composer (UI-1: static rendering + in-memory
  editing only). Stood up as a new route per UI_PLAN.md §6: the existing
  /loop cockpit (health, autonomy ladder, self-prompt strategy) is a
  different surface and is left untouched.

  Nothing here runs, persists, or writes to the backend. The stack is a
  client-only ordered list (`stores/stack.ts`); guardrails/evals popovers,
  live controls, and output attachment are later slices (UI-2/UI-3/UI-4).
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import Panel from '$lib/components/ui/Panel.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Dropdown from '$lib/components/ui/Dropdown.svelte';
  import StackComposer from '$lib/components/stacks/StackComposer.svelte';
  import StackCard from '$lib/components/stacks/StackCard.svelte';
  import { stack } from '$lib/stores/stack';
  import { stackDefaults, AUTONOMY_OPTIONS } from '$lib/stores/stackDefaults';
  import { MODEL_OPTIONS, EFFORT_OPTIONS, type Option } from '$lib/stores/controls';
  import { listRepos } from '$lib/api';

  let repoOptions: Option[] = [{ value: '', label: 'auto' }];

  onMount(async () => {
    try {
      const { repos } = await listRepos();
      if (repos.length) {
        repoOptions = [
          { value: '', label: 'auto' },
          ...repos.map((r) => ({ value: r, label: r }))
        ];
      }
    } catch {
      // Repo listing is best-effort chrome — the composer works with the
      // "auto" default if /api/repos is unreachable (e.g. static preview).
    }
  });
</script>

<div class="max-w-2xl mx-auto px-6 py-8 space-y-6">
  <div>
    <h1 class="font-display text-2xl">Loop Stack</h1>
    <p class="font-mono text-[11px] uppercase tracking-widest opacity-50 mt-1">
      compose a stack · static preview this slice · nothing runs yet
    </p>
  </div>

  <Panel title="Stack defaults" subtitle="model · effort · repo · autonomy for every card added below">
    <div class="selrow">
      <Dropdown dense label="model" bind:value={$stackDefaults.model} options={MODEL_OPTIONS} />
      <Dropdown dense label="effort" bind:value={$stackDefaults.effort} options={EFFORT_OPTIONS} />
      <Dropdown dense label="repo" bind:value={$stackDefaults.repo} options={repoOptions} />
      <div class="autonomy">
        <Dropdown
          dense
          label="autonomy"
          bind:value={$stackDefaults.autonomy}
          options={AUTONOMY_OPTIONS}
        />
      </div>
    </div>
  </Panel>

  <StackComposer />

  <div class="stackline">
    {#if $stack.length === 0}
      <EmptyState title="Stack is empty" detail="Add a prompt above — it lands at the top." />
    {:else}
      {#each $stack as card, i (card.id)}
        <StackCard {card} isNext={i === $stack.length - 1} />
        {#if i < $stack.length - 1}
          <div class="gap"><div class="line"></div></div>
        {/if}
      {/each}
    {/if}
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
  .stackline {
    display: flex;
    flex-direction: column;
  }
  .gap {
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .gap .line {
    width: 1px;
    height: 100%;
    background: rgba(255, 255, 255, 0.11);
  }
</style>
