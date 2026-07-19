<!--
  StackConfigPopover — content rendered inside `Popover` for the stack
  control dock's sliders button: the stack's own default model/effort/repo/
  branch/autonomy/permission_mode, edited directly (not an override of
  something else — the stack IS where these defaults live; every loop's
  `ConfigDrawer.svelte` override falls back to exactly this object).
  `model`/`effort`/`repo`/`permission_mode` are WIRED (resolved into every
  loop's real `CreateTaskOptions` at the payload step,
  `stores/stack.ts::cardToTaskPayload`); `autonomy` is client-only, same
  as at loop scope. `branch` reaches the server as a planning constraint and
  offers the selected repo's real branches (`stores/branches.ts`). Reuses
  `Dropdown.svelte` the same way `ConfigDrawer.svelte` does — not a fork, a
  second mount of the same primitive over stack-scoped data.
-->
<script lang="ts">
  import {
    type StackDefaults,
    AUTONOMY_OPTIONS,
    PERMISSION_MODE_OPTIONS,
    resolveBranch
  } from '$lib/stores/stackDefaults';
  import { branchesByRepo, branchOptionsFor, ensureBranches } from '$lib/stores/branches';
  import { type Option } from '$lib/stores/controls';
  import { modelCatalog, modelOptionsFrom, effortOptionsFor, ensureModelCatalog } from '$lib/stores/modelCatalog';
  import { closePopover } from './Popover.svelte';
  import Dropdown from '$lib/components/ui/Dropdown.svelte';
  import { ICONS } from './icons';

  export let defaults: StackDefaults;
  export let onChange: (patch: Partial<StackDefaults>) => void;
  export let repoOptions: Option[] = [];

  $: effectiveRepoOptions = repoOptions.length ? repoOptions : [{ value: defaults.repo, label: defaults.repo || 'auto' }];

  $: void ensureModelCatalog();
  $: modelOptions = modelOptionsFrom($modelCatalog);
  $: effortOptions = effortOptionsFor($modelCatalog, defaults.model);

  $: void ensureBranches(defaults.repo);
  $: branchOptions = branchOptionsFor($branchesByRepo, defaults.repo);
  // Store what we show — see the same note in `ConfigDrawer.svelte`.
  $: resolved = resolveBranch(defaults.branch, branchOptions.map((o) => o.value), $branchesByRepo[defaults.repo]?.head ?? '');
  $: if (resolved !== defaults.branch) onChange({ branch: resolved });
</script>

<div class="ph">{@html ICONS.sliders}default config · every loop inherits</div>
<div class="pbody">
  <div class="cfgrow model">
    <Dropdown dense label="model" icon={ICONS.cpu} value={defaults.model} options={modelOptions} on:change={(e) => onChange({ model: e.detail })} />
  </div>
  <div class="cfgrow effort">
    <Dropdown dense label="effort" icon={ICONS.gauge} value={defaults.effort} options={effortOptions} on:change={(e) => onChange({ effort: e.detail })} />
  </div>
  <div class="cfgrow repo">
    <Dropdown dense searchable label="repo" icon={ICONS.folder} value={defaults.repo} options={effectiveRepoOptions} on:change={(e) => onChange({ repo: e.detail })} />
  </div>
  <div class="cfgrow branch">
    <Dropdown dense label="branch" icon={ICONS.branch} value={resolved} options={branchOptions} on:change={(e) => onChange({ branch: e.detail })} />
  </div>
  <div class="cfgrow autonomy">
    <Dropdown dense label="autonomy" icon={ICONS.ladder} value={defaults.autonomy} options={AUTONOMY_OPTIONS} on:change={(e) => onChange({ autonomy: e.detail })} />
  </div>
  <div class="cfgrow permission-mode">
    <Dropdown dense label="permission" icon={ICONS.lock} value={defaults.permission_mode} options={PERMISSION_MODE_OPTIONS} on:change={(e) => onChange({ permission_mode: e.detail })} />
  </div>
</div>
<div class="popfoot">
  <button class="apply" on:click={closePopover}>done</button>
</div>

<style>
  .cfgrow {
    margin-bottom: 9px;
  }
  .cfgrow:last-child {
    margin-bottom: 0;
  }
  /* Per-field accent for the leading icon — matches the mockup's icon colours
     and the loop-scope ConfigDrawer. */
  .cfgrow.model {
    --konjo-accent-rgb: 0 212 255;
  }
  .cfgrow.effort {
    --konjo-accent-rgb: 255 69 0;
  }
  .cfgrow.repo {
    --konjo-accent-rgb: 255 204 0;
  }
  .cfgrow.branch {
    --konjo-accent-rgb: 0 255 157;
  }
  .cfgrow.autonomy {
    --konjo-accent-rgb: 183 155 255;
  }
  .cfgrow.permission-mode {
    --konjo-accent-rgb: 255 90 90;
  }
</style>
