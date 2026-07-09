<!--
  StackConfigPopover — content rendered inside `Popover` for the stack
  control dock's sliders button: the stack's own default model/effort/repo/
  branch/autonomy, edited directly (not an override of something else — the
  stack IS where these defaults live; every loop's `ConfigDrawer.svelte`
  override falls back to exactly this object). `model`/`effort`/`repo` are
  WIRED (resolved into every loop's real `CreateTaskOptions` at the payload
  step, `stores/stack.ts::cardToTaskPayload`); `branch`/`autonomy` are
  client-only, same as at loop scope. Reuses `Dropdown.svelte` the same way
  `ConfigDrawer.svelte` does — not a fork, a second mount of the same
  primitive over stack-scoped data.
-->
<script lang="ts">
  import { type StackDefaults, AUTONOMY_OPTIONS, BRANCH_OPTIONS } from '$lib/stores/stackDefaults';
  import { MODEL_OPTIONS, EFFORT_OPTIONS, type Option } from '$lib/stores/controls';
  import { closePopover } from './Popover.svelte';
  import Dropdown from '$lib/components/ui/Dropdown.svelte';
  import { ICONS } from './icons';

  export let defaults: StackDefaults;
  export let onChange: (patch: Partial<StackDefaults>) => void;
  export let repoOptions: Option[] = [];

  $: effectiveRepoOptions = repoOptions.length ? repoOptions : [{ value: defaults.repo, label: defaults.repo || 'auto' }];
</script>

<div class="ph">{@html ICONS.sliders}default config · every loop inherits</div>
<div class="pbody">
  <div class="cfgrow">
    <Dropdown dense label="model" value={defaults.model} options={MODEL_OPTIONS} on:change={(e) => onChange({ model: e.detail })} />
  </div>
  <div class="cfgrow">
    <Dropdown dense label="effort" value={defaults.effort} options={EFFORT_OPTIONS} on:change={(e) => onChange({ effort: e.detail })} />
  </div>
  <div class="cfgrow">
    <Dropdown dense label="repo" value={defaults.repo} options={effectiveRepoOptions} on:change={(e) => onChange({ repo: e.detail })} />
  </div>
  <div class="cfgrow">
    <Dropdown dense label="branch" value={defaults.branch} options={BRANCH_OPTIONS} on:change={(e) => onChange({ branch: e.detail })} />
  </div>
  <div class="cfgrow">
    <Dropdown dense label="autonomy" value={defaults.autonomy} options={AUTONOMY_OPTIONS} on:change={(e) => onChange({ autonomy: e.detail })} />
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
</style>
