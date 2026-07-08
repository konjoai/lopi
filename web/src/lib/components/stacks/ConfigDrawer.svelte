<!--
  ConfigDrawer — the sliders-button inline drawer with five per-loop
  overrides of the pane defaults. `model`/`effort`/`repo` are WIRED (real
  `CreateTaskRequest` fields); `branch`/`autonomy` are client-only —
  TODO(backend). Built on the shared `Dropdown.svelte`, not a popover.
-->
<script lang="ts">
  import { type StackCard as StackCardT, type CardConfig, updateCardInPane } from '$lib/stores/stack';
  import { type StackDefaults, AUTONOMY_OPTIONS, BRANCH_OPTIONS } from '$lib/stores/stackDefaults';
  import { MODEL_OPTIONS, EFFORT_OPTIONS, type Option } from '$lib/stores/controls';
  import Dropdown from '$lib/components/ui/Dropdown.svelte';

  export let card: StackCardT;
  export let paneKey: string;
  export let paneDefaults: StackDefaults;
  export let repoOptions: Option[] = [];

  function patchConfig(patch: Partial<CardConfig>) {
    updateCardInPane(paneKey, card.id, { config: { ...card.config, ...patch } });
  }

  $: effectiveRepoOptions = repoOptions.length
    ? repoOptions
    : [{ value: paneDefaults.repo, label: paneDefaults.repo || 'auto' }];
</script>

<div class="cfgdrawer">
  <div class="chip model">
    <Dropdown
      dense
      label="model"
      value={card.config.model ?? paneDefaults.model}
      options={MODEL_OPTIONS}
      on:change={(e) => patchConfig({ model: e.detail })}
    />
  </div>
  <div class="chip effort">
    <Dropdown
      dense
      label="effort"
      value={card.config.effort ?? paneDefaults.effort}
      options={EFFORT_OPTIONS}
      on:change={(e) => patchConfig({ effort: e.detail })}
    />
  </div>
  <div class="chip repo">
    <Dropdown
      dense
      label="repo"
      value={card.config.repo ?? paneDefaults.repo}
      options={effectiveRepoOptions}
      on:change={(e) => patchConfig({ repo: e.detail })}
    />
  </div>
  <div class="chip branch">
    <Dropdown
      dense
      label="branch"
      value={card.config.branch ?? paneDefaults.branch}
      options={BRANCH_OPTIONS}
      on:change={(e) => patchConfig({ branch: e.detail })}
    />
  </div>
  <div class="chip autonomy">
    <Dropdown
      dense
      label="autonomy"
      value={card.config.autonomy ?? paneDefaults.autonomy}
      options={AUTONOMY_OPTIONS}
      on:change={(e) => patchConfig({ autonomy: e.detail })}
    />
  </div>
</div>

<style>
  .cfgdrawer {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    justify-content: flex-start;
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .chip {
    flex: 0 0 auto;
  }
  /* Each selector's accent matches the mockup's per-field icon color —
     Dropdown.svelte reads --konjo-accent-rgb for its hover/open state. */
  .chip.model {
    --konjo-accent-rgb: 0 212 255;
  }
  .chip.effort {
    --konjo-accent-rgb: 255 69 0;
  }
  .chip.repo {
    --konjo-accent-rgb: 255 204 0;
  }
  .chip.branch {
    --konjo-accent-rgb: 0 255 157;
  }
  .chip.autonomy {
    --konjo-accent-rgb: 183 155 255;
  }
</style>
