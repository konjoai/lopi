<!--
  SchedulePopover — content rendered inside `Popover` for the cardbar's
  cyan schedule button. `cron.raw` is WIRED — it mirrors `ScheduleEntry.cron`
  (`crates/lopi-core/src/config.rs`); the preset fields two-way-sync with it.
-->
<script lang="ts">
  import {
    type StackCard as StackCardT,
    type CronConfig,
    type CronFreq,
    type Dow,
    buildCronString,
    cronHuman,
    computeNextRuns,
    updateCardInPane
  } from '$lib/stores/stack';
  import { closePopover } from './Popover.svelte';
  import Dropdown from '$lib/components/ui/Dropdown.svelte';
  import Combo from './Combo.svelte';
  import Toggle from './Toggle.svelte';
  import { ICONS } from './icons';

  export let card: StackCardT;
  export let paneKey: string;

  const FREQS: CronFreq[] = ['every minute', 'hourly', 'daily', 'weekly', 'custom'];
  const DOWS: Dow[] = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
  const DOW_OPTIONS = DOWS.map((d) => ({ value: d, label: d }));
  const HOURS = Array.from({ length: 12 }, (_, i) => i + 1);
  const MINUTES = [0, 15, 30, 45];

  function patchCron(patch: Partial<CronConfig>) {
    const next: CronConfig = { ...card.cron, ...patch };
    if (next.freq !== 'custom') next.raw = buildCronString(next);
    updateCardInPane(paneKey, card.id, { cron: next });
  }

  function toggleScheduled() {
    updateCardInPane(paneKey, card.id, { scheduled: !card.scheduled });
  }

  function onRawInput(e: Event) {
    const raw = (e.target as HTMLInputElement).value;
    updateCardInPane(paneKey, card.id, { cron: { ...card.cron, freq: 'custom', raw } });
  }

  function onDowChange(e: CustomEvent<string>) {
    patchCron({ dow: e.detail as Dow });
  }

  $: cronExpr = buildCronString(card.cron);
  $: human = cronHuman(card.cron);
  $: rawSize = Math.min(Math.max(cronExpr.length, 9), 34);
  $: runs = card.scheduled ? computeNextRuns(cronExpr, new Date(), 3) : [];

  function formatRun(d: Date): string {
    return d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });
  }
</script>

<div class="ph">{@html ICONS.cron}schedule</div>
<div class="pbody">
  <div class="enrow">
    <Toggle on={card.scheduled} onToggle={toggleScheduled} accent="ice" />
    <span>run on a schedule</span>
  </div>

  {#if card.scheduled}
    <div class="freqrow">
      {#each FREQS as f (f)}
        <button type="button" class="freq" class:on={card.cron.freq === f} on:click={() => patchCron({ freq: f })}>
          {f}
        </button>
      {/each}
    </div>

    {#if card.cron.freq === 'weekly'}
      <div class="detrow">
        <span>on</span>
        <Dropdown dense value={card.cron.dow} options={DOW_OPTIONS} on:change={onDowChange} />
        <span>at</span>
        <Combo value={card.cron.hour12} options={HOURS} min={1} max={12} onChange={(n) => patchCron({ hour12: n })} />
        <span class="colon">:</span>
        <Combo value={card.cron.min} options={MINUTES} min={0} max={59} onChange={(n) => patchCron({ min: n })} />
        <span class="ampm" class:pm={card.cron.ampm === 'PM'}>
          <button type="button" class="ap" class:on={card.cron.ampm === 'AM'} on:click={() => patchCron({ ampm: 'AM' })}>AM</button>
          <button type="button" class="ap" class:on={card.cron.ampm === 'PM'} on:click={() => patchCron({ ampm: 'PM' })}>PM</button>
        </span>
      </div>
    {:else if card.cron.freq === 'daily'}
      <div class="detrow">
        <span>at</span>
        <Combo value={card.cron.hour12} options={HOURS} min={1} max={12} onChange={(n) => patchCron({ hour12: n })} />
        <span class="colon">:</span>
        <Combo value={card.cron.min} options={MINUTES} min={0} max={59} onChange={(n) => patchCron({ min: n })} />
        <span class="ampm" class:pm={card.cron.ampm === 'PM'}>
          <button type="button" class="ap" class:on={card.cron.ampm === 'AM'} on:click={() => patchCron({ ampm: 'AM' })}>AM</button>
          <button type="button" class="ap" class:on={card.cron.ampm === 'PM'} on:click={() => patchCron({ ampm: 'PM' })}>PM</button>
        </span>
      </div>
    {:else if card.cron.freq === 'hourly'}
      <div class="detrow">
        <span>at minute</span>
        <Combo value={card.cron.min} options={MINUTES} min={0} max={59} onChange={(n) => patchCron({ min: n })} />
      </div>
    {/if}

    <div class="rawrow">
      <span class="rl">cron</span>
      <input value={cronExpr} size={rawSize} on:input={onRawInput} spellcheck="false" />
    </div>
    <div class="human">{human} → <b>{cronExpr}</b></div>
  {/if}
</div>

{#if card.scheduled}
  <div class="schedfoot">
    <div class="nextruns">
      <span class="nrl">next runs:</span>
      {#if runs.length}
        <ul>
          {#each runs as r (r.getTime())}
            <li>{formatRun(r)}</li>
          {/each}
        </ul>
      {/if}
    </div>
    <button class="apply" on:click={closePopover}>done</button>
  </div>
{:else}
  <div class="popfoot">
    <button class="apply" on:click={closePopover}>done</button>
  </div>
{/if}

<style>
  .enrow {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-bottom: 11px;
    font-size: 11px;
    color: var(--konjo-paper, #f5f5f5);
  }
  .freqrow {
    display: flex;
    gap: 5px;
    flex-wrap: wrap;
    margin-bottom: 11px;
  }
  .freq {
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    color: rgba(245, 245, 245, 0.46);
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 16px;
    padding: 5px 11px;
    cursor: pointer;
    background: transparent;
  }
  .freq.on {
    background: rgba(0, 212, 255, 0.14);
    color: var(--konjo-ice);
    border-color: rgba(0, 212, 255, 0.45);
  }
  .detrow {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-bottom: 11px;
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    color: rgba(245, 245, 245, 0.66);
    flex-wrap: wrap;
  }
  .colon {
    color: rgba(245, 245, 245, 0.66);
    font-weight: 700;
  }
  .ampm {
    display: inline-flex;
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 20px;
    overflow: hidden;
    background: rgba(255, 255, 255, 0.03);
    position: relative;
  }
  .ampm .ap {
    padding: 5px 11px;
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    color: rgba(245, 245, 245, 0.66);
    cursor: pointer;
    transition: 0.16s;
    position: relative;
    z-index: 1;
    border: none;
    background: transparent;
  }
  .ampm .ap.on {
    color: #04141c;
  }
  .ampm::before {
    content: '';
    position: absolute;
    top: 2px;
    bottom: 2px;
    left: 2px;
    width: calc(50% - 2px);
    border-radius: 18px;
    background: var(--konjo-ice);
    transition: transform 0.18s cubic-bezier(0.4, 0, 0.2, 1);
    z-index: 0;
  }
  .ampm.pm::before {
    transform: translateX(100%);
  }
  .rawrow {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 4px;
  }
  .rawrow .rl {
    font-family: var(--font-mono, monospace);
    font-size: 8px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: rgba(245, 245, 245, 0.66);
    flex: 0 0 auto;
  }
  .rawrow input {
    background: rgba(255, 255, 255, 0.03);
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 5px;
    padding: 5px 8px;
    color: var(--konjo-ice);
    font-family: var(--font-mono, monospace);
    font-size: 10.5px;
    letter-spacing: 0.05em;
    max-width: 100%;
  }
  .rawrow input:focus {
    outline: none;
    border-color: rgba(0, 212, 255, 0.5);
  }
  .human {
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    color: rgba(245, 245, 245, 0.46);
    margin-top: 7px;
  }
  .human :global(b) {
    color: var(--konjo-ice);
  }
  .schedfoot {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    gap: 14px;
    padding: 10px 13px;
    border-top: 1px solid rgba(255, 255, 255, 0.05);
  }
  .nextruns {
    font-family: var(--font-mono, monospace);
    font-size: 9px;
    color: rgba(245, 245, 245, 0.46);
  }
  .nextruns .nrl {
    color: rgba(245, 245, 245, 0.66);
  }
  .nextruns ul {
    list-style: none;
    margin: 4px 0 0;
    padding: 0;
  }
  .nextruns li {
    position: relative;
    padding-left: 12px;
    line-height: 1.7;
    color: rgba(245, 245, 245, 0.46);
  }
  .nextruns li::before {
    content: '–';
    position: absolute;
    left: 0;
    color: var(--konjo-ice);
  }
</style>
