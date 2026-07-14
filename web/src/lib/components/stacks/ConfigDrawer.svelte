<!--
  ConfigDrawer — the sliders-button inline drawer with five per-loop
  overrides of the pane defaults. `model`/`effort`/`repo` are WIRED (real
  `CreateTaskRequest` fields); `autonomy` is client-only. `branch` reaches the
  server as a planning constraint (`paneSubmitPayload`) and its options are the
  selected repo's real branches, fetched via `stores/branches.ts`. Built on
  `Dropdown.svelte`, not a popover.
-->
<script lang="ts">
  import { type StackCard as StackCardT, type CardConfig, updateCardInPane } from '$lib/stores/stack';
  import { type StackDefaults, AUTONOMY_OPTIONS, resolveBranch } from '$lib/stores/stackDefaults';
  import { branchesByRepo, branchOptionsFor, ensureBranches } from '$lib/stores/branches';
  import { MODEL_OPTIONS, EFFORT_OPTIONS, type Option } from '$lib/stores/controls';
  import Dropdown from '$lib/components/ui/Dropdown.svelte';
  import { ICONS } from './icons';

  export let card: StackCardT;
  export let paneKey: string;
  export let paneDefaults: StackDefaults;
  export let repoOptions: Option[] = [];
  /** Injected card-patch writer (Creation-Flow-1). A draft card is not in
   *  `pane.cards`, so its config edits must route to the pane's draft, not
   *  `updateCardInPane` (which would no-op on an id it can't find). When null,
   *  falls back to the committed-card write path for standalone use. */
  export let onWrite: ((patch: Partial<StackCardT>) => void) | null = null;

  function patchConfig(patch: Partial<CardConfig>) {
    const next = { config: { ...card.config, ...patch } };
    if (onWrite) onWrite(next);
    else updateCardInPane(paneKey, card.id, next);
  }

  $: effectiveRepoOptions = repoOptions.length
    ? repoOptions
    : [{ value: paneDefaults.repo, label: paneDefaults.repo || 'auto' }];

  // This card's own repo — not the pane's — drives its branch list.
  $: repo = card.config.repo ?? paneDefaults.repo;
  $: void ensureBranches(repo);
  $: branchOptions = branchOptionsFor($branchesByRepo, repo);
  $: branch = card.config.branch ?? paneDefaults.branch;
  // Store what we show. Displaying a resolved branch while leaving a stale one
  // in `config` would launch against a target the user never saw. Converges:
  // once patched, `branch` equals `resolved` and this stops firing.
  $: resolved = resolveBranch(branch, branchOptions.map((o) => o.value), $branchesByRepo[repo]?.head ?? '');
  $: if (resolved !== branch) patchConfig({ branch: resolved });
</script>

<div class="cfgdrawer">
  <div class="chip model">
    <Dropdown
      dense
      label="model"
      icon={ICONS.cpu}
      value={card.config.model ?? paneDefaults.model}
      options={MODEL_OPTIONS}
      on:change={(e) => patchConfig({ model: e.detail })}
    />
  </div>
  <div class="chip effort">
    <Dropdown
      dense
      label="effort"
      icon={ICONS.gauge}
      value={card.config.effort ?? paneDefaults.effort}
      options={EFFORT_OPTIONS}
      on:change={(e) => patchConfig({ effort: e.detail })}
    />
  </div>
  <div class="chip repo">
    <Dropdown
      dense
      searchable
      label="repo"
      icon={ICONS.folder}
      value={card.config.repo ?? paneDefaults.repo}
      options={effectiveRepoOptions}
      on:change={(e) => patchConfig({ repo: e.detail })}
    />
  </div>
  <div class="chip branch">
    <Dropdown
      dense
      label="branch"
      icon={ICONS.branch}
      value={resolved}
      options={branchOptions}
      on:change={(e) => patchConfig({ branch: e.detail })}
    />
  </div>
  <div class="chip autonomy">
    <Dropdown
      dense
      label="autonomy"
      icon={ICONS.ladder}
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
