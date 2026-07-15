<!--
  MaxxPopover — content rendered inside `Popover` (kind="max") for the
  cardbar's flame MAXX button. Unlike SchedulePopover, the enable toggle is
  wired to real `/api/maxx` CRUD (create-on-first-enable, then enable/
  disable) rather than staying client-local until stack submit — the sprint
  spec calls for that directly. The two "run" conditions (quiet hours,
  headroom gate) are fixed policy shown as descriptive text, not editable
  fields, in this sprint — only the top-level enable toggle is interactive;
  see MAXX Phase 2's locked design (two-item list, not prose/widgets).
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { createMaxx, enableMaxx, disableMaxx, getQuota, ApiError, type QuotaSnapshot, type QuotaWindow } from '$lib/api';
  import { closePopover } from './Popover.svelte';
  import Toggle from './Toggle.svelte';
  import { ICONS } from './icons';
  import type { MaxxConfig } from '$lib/stores/stack';

  export let maxx: MaxxConfig;
  export let entryId: string | undefined;
  export let goal: string;
  export let repo: string | undefined;
  /** Called after a toggle's CRUD call settles — patches the card with the
   *  new `enabled` state and (on first enable) the freshly created entry id. */
  export let onToggled: (next: { enabled: boolean; entryId: string | undefined }) => void;

  let busy = false;
  let error = '';
  let quota: QuotaSnapshot | null = null;
  let quotaError = '';

  onMount(async () => {
    try {
      quota = await getQuota();
    } catch (e) {
      quotaError = e instanceof ApiError ? e.message : 'failed to load quota';
    }
  });

  async function toggle() {
    if (busy) return;
    busy = true;
    error = '';
    const next = !maxx.enabled;
    try {
      let id = entryId;
      if (next) {
        if (id) {
          await enableMaxx(id);
        } else {
          const created = await createMaxx({
            name: goal.trim() ? goal.slice(0, 60) : 'maxx entry',
            goal,
            repo,
            headroom_gate: maxx.headroomGate,
            quiet_hours: maxx.quietHours,
            windows: maxx.windows,
            enabled: true
          });
          id = created.id;
        }
      } else if (id) {
        await disableMaxx(id);
      }
      onToggled({ enabled: next, entryId: id });
    } catch (e) {
      error = e instanceof ApiError ? e.message : 'request failed';
    } finally {
      busy = false;
    }
  }

  /** `0..23` local hour → `"11PM"`/`"7AM"`-style 12-hour label. */
  function fmtHour12(h: number): string {
    const period = h < 12 ? 'AM' : 'PM';
    const h12 = h % 12 === 0 ? 12 : h % 12;
    return `${h12}${period}`;
  }

  function pct(w: QuotaWindow | null): number {
    return w ? Math.round(w.utilization * 100) : 0;
  }

  /** "resets in 2h10m" from a unix-seconds `resets_at`. */
  function resetIn(resetsAt: number): string {
    const secs = Math.max(0, resetsAt - Math.floor(Date.now() / 1000));
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    return h > 0 ? `resets in ${h}h${m}m` : `resets in ${m}m`;
  }

  /** "resets on Thu 9AM" from a unix-seconds `resets_at`. */
  function resetOn(resetsAt: number): string {
    const d = new Date(resetsAt * 1000);
    const weekday = d.toLocaleDateString(undefined, { weekday: 'short' });
    return `resets on ${weekday} ${fmtHour12(d.getHours())}`;
  }

  function windowText(w: QuotaWindow | null, kind: 'five_hour' | 'seven_day'): string {
    if (!w) return 'no data yet';
    const resetText = w.resets_at === null ? 'reset time unknown' : kind === 'five_hour' ? resetIn(w.resets_at) : resetOn(w.resets_at);
    return `${pct(w)}% · ${resetText}`;
  }
</script>

<div class="ph">{@html ICONS.bolt}MAXX</div>
<div class="pbody">
  <div class="enrow">
    <Toggle on={maxx.enabled} onToggle={toggle} accent="flame" />
    <span>enable MAXX</span>
  </div>
  {#if error}<div class="err">{error}</div>{/if}

  <div class="fieldlbl">run</div>
  <ul class="runlist">
    <li>After hours <b>{fmtHour12(maxx.quietHours[0])}–{fmtHour12(maxx.quietHours[1])}</b></li>
    <li>Nearing quota reset with high headroom</li>
  </ul>

  <div class="fieldlbl">current quota</div>
  {#if quotaError}
    <div class="err">{quotaError}</div>
  {:else}
    <div class="qbar-row">
      <div class="qbar-top"><span>5h window</span><span>{windowText(quota?.five_hour ?? null, 'five_hour')}</span></div>
      <div class="qbar-track"><div class="qbar-fill ice" style="width:{pct(quota?.five_hour ?? null)}%"></div></div>
    </div>
    <div class="qbar-row">
      <div class="qbar-top"><span>7d window</span><span>{windowText(quota?.seven_day ?? null, 'seven_day')}</span></div>
      <div class="qbar-track"><div class="qbar-fill jade" style="width:{pct(quota?.seven_day ?? null)}%"></div></div>
    </div>
  {/if}
</div>
<div class="popfoot">
  <button class="apply" on:click={closePopover}>done</button>
</div>

<style>
  .enrow {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-bottom: 11px;
    font-size: 11px;
    color: var(--konjo-paper, #f5f5f5);
  }
  .err {
    font-size: 9px;
    color: var(--konjo-rose, #ff0066);
    margin-bottom: 9px;
  }
  .fieldlbl {
    font-size: 8.5px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: rgba(245, 245, 245, 0.4);
    margin: 10px 0 5px;
  }
  .fieldlbl:first-of-type {
    margin-top: 0;
  }
  .runlist {
    margin: 3px 0 0;
    padding-left: 14px;
    list-style: none;
  }
  .runlist li {
    position: relative;
    font-size: 9px;
    color: rgba(245, 245, 245, 0.6);
    line-height: 1.7;
  }
  .runlist li::before {
    content: '·';
    position: absolute;
    left: -12px;
    color: var(--konjo-flame);
    font-weight: 700;
  }
  .runlist :global(b) {
    color: var(--konjo-flame);
  }
  .qbar-row {
    margin: 8px 0;
  }
  .qbar-top {
    display: flex;
    justify-content: space-between;
    font-size: 8.5px;
    color: rgba(245, 245, 245, 0.46);
    margin-bottom: 4px;
  }
  .qbar-track {
    height: 5px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.08);
    overflow: hidden;
  }
  .qbar-fill {
    height: 100%;
    border-radius: 3px;
  }
  .qbar-fill.ice {
    background: var(--konjo-ice);
  }
  .qbar-fill.jade {
    background: var(--konjo-jade);
  }
</style>
