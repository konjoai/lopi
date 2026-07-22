<script lang="ts">
  import { onMount } from 'svelte';
  import {
    listSchedules,
    createSchedule,
    updateSchedule,
    deleteSchedule,
    enableSchedule,
    disableSchedule,
    runScheduleNow,
    type Schedule,
    type ScheduleBody
  } from '$lib/api';
  import Panel from '$lib/components/ui/Panel.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';

  let schedules: Schedule[] = [];
  let loading = true;
  let loadError = '';
  let actionError = '';
  let flash = '';

  // Form state — doubles as the edit form when `editingId` is set.
  let editingId: string | null = null;
  let formOpen = false;
  let form: ScheduleBody = blankForm();
  let saving = false;

  function blankForm(): ScheduleBody {
    return { name: '', cron: '0 0 * * * *', goal: '', repo: '', priority: 'normal', enabled: true };
  }

  async function refresh() {
    try {
      const r = await listSchedules();
      schedules = r.schedules;
      loadError = '';
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'failed to load';
    } finally {
      loading = false;
    }
  }

  function openCreate() {
    editingId = null;
    form = blankForm();
    formOpen = true;
    actionError = '';
  }

  function openEdit(s: Schedule) {
    editingId = s.id;
    form = {
      name: s.name,
      cron: s.cron,
      goal: s.goal,
      repo: s.repo ?? '',
      priority: s.priority ?? 'normal',
      enabled: s.enabled
    };
    formOpen = true;
    actionError = '';
  }

  async function save() {
    if (!form.name.trim() || !form.cron.trim() || !form.goal.trim() || saving) return;
    saving = true;
    actionError = '';
    try {
      if (editingId) await updateSchedule(editingId, form);
      else await createSchedule(form);
      formOpen = false;
      flash = editingId ? 'schedule updated' : 'schedule created';
      setTimeout(() => (flash = ''), 2500);
      await refresh();
    } catch (e) {
      actionError = e instanceof Error ? e.message : 'save failed';
    } finally {
      saving = false;
    }
  }

  async function act(fn: () => Promise<unknown>, okFlash: string) {
    actionError = '';
    try {
      await fn();
      flash = okFlash;
      setTimeout(() => (flash = ''), 2500);
      await refresh();
    } catch (e) {
      actionError = e instanceof Error ? e.message : 'action failed';
    }
  }

  function fmtTime(iso: string | undefined | null): string {
    if (!iso) return '—';
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? String(iso) : d.toLocaleString();
  }

  onMount(() => {
    refresh();
    const t = setInterval(refresh, 15000);
    return () => clearInterval(t);
  });
</script>

<svelte:head><title>lopi · schedules</title></svelte:head>

<div class="max-w-6xl mx-auto px-6 py-8 space-y-6">
  <!-- Header -->
  <div class="flex items-end justify-between flex-wrap gap-4">
    <div>
      <h1 class="font-display text-2xl">Scheduling</h1>
      <p class="font-mono text-[11px] uppercase tracking-widest opacity-50 mt-1">
        cron-driven agent runs · {schedules.length} configured
      </p>
    </div>
  </div>

  <Panel
    title="All Schedules"
    subtitle="click run to fire one now, or edit to change its cron"
  >
    <svelte:fragment slot="actions">
      {#if flash}
        <span class="font-mono text-[10px] uppercase tracking-widest text-konjo-jade animate-pulse">
          {flash}
        </span>
      {/if}
      <button
        type="button"
        on:click={openCreate}
        class="px-3 py-1 rounded font-mono text-[10px] uppercase tracking-widest bg-konjo-accent/10 text-konjo-accent border border-konjo-accent/40 hover:bg-konjo-accent/20 transition-colors"
      >
        + new
      </button>
    </svelte:fragment>

    {#if actionError}
      <p class="mb-3 font-mono text-xs" style:color="var(--konjo-rose)">{actionError}</p>
    {/if}

    {#if loading}
      <EmptyState title="loading…" />
    {:else if loadError}
      <EmptyState error title="backend unreachable" detail={loadError} />
    {:else if schedules.length === 0 && !formOpen}
      <EmptyState title="no schedules" detail="create one to run agents on a cadence" />
    {:else}
      <div class="space-y-2">
        {#each schedules as s (s.id)}
          <div
            class="rounded border px-3 py-2.5 transition-colors"
            class:border-white={false}
            style:border-color={s.enabled ? 'rgb(var(--konjo-accent-rgb) / 0.25)' : 'rgba(255,255,255,0.08)'}
            style:opacity={s.enabled ? 1 : 0.55}
          >
            <div class="flex items-start gap-3">
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class="font-display text-sm font-bold">{s.name}</span>
                  <code
                    class="font-mono text-[10px] px-1.5 py-0.5 rounded bg-black/40 border border-white/10"
                  >
                    {s.cron}
                  </code>
                  {#if !s.enabled}
                    <span class="font-mono text-[9px] uppercase tracking-widest opacity-50">
                      paused
                    </span>
                  {/if}
                </div>
                <div class="font-mono text-xs opacity-70 mt-1 truncate">{s.goal}</div>
                <div class="font-mono text-[10px] opacity-40 mt-1">
                  next: {s.next_runs.length > 0 ? fmtTime(s.next_runs[0]) : '—'}
                  · last: {s.last_run ? `${fmtTime(s.last_run.fired_at)} (${s.last_run.outcome ?? '…'})` : 'never'}
                  {#if s.repo}· {s.repo}{/if}
                </div>
              </div>
              <div class="flex gap-2 flex-shrink-0 font-mono text-[10px] uppercase tracking-widest">
                <button
                  type="button"
                  on:click={() => act(() => runScheduleNow(s.id), `${s.name} queued`)}
                  class="px-2 py-1 rounded border border-white/10 text-konjo-jade hover:border-konjo-jade/50 hover:bg-konjo-jade/10 transition-colors"
                  title="Run now"
                >
                  ▶ run
                </button>
                <button
                  type="button"
                  on:click={() =>
                    act(
                      () => (s.enabled ? disableSchedule(s.id) : enableSchedule(s.id)),
                      s.enabled ? `${s.name} paused` : `${s.name} enabled`
                    )}
                  class="px-2 py-1 rounded border border-white/10 text-konjo-sun hover:border-konjo-sun/50 hover:bg-konjo-sun/10 transition-colors"
                >
                  {s.enabled ? 'pause' : 'enable'}
                </button>
                <button
                  type="button"
                  on:click={() => openEdit(s)}
                  class="px-2 py-1 rounded border border-white/10 text-konjo-accent hover:border-konjo-accent/50 hover:bg-konjo-accent/10 transition-colors"
                >
                  edit
                </button>
                <button
                  type="button"
                  on:click={() => act(() => deleteSchedule(s.id), `${s.name} deleted`)}
                  class="px-2 py-1 rounded border border-white/10 text-konjo-rose hover:border-konjo-rose/50 hover:bg-konjo-rose/10 transition-colors"
                >
                  delete
                </button>
              </div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </Panel>

  {#if formOpen}
    <Panel title={editingId ? 'Edit schedule' : 'New schedule'} subtitle="6-field cron: sec min hour day month weekday">
      <svelte:fragment slot="actions">
        <button
          type="button"
          on:click={() => (formOpen = false)}
          class="w-5 h-5 flex items-center justify-center bg-white/10 hover:bg-white/25 text-white/60 hover:text-white rounded-full text-[10px] font-bold transition-colors"
          aria-label="Close form"
        >
          ✕
        </button>
      </svelte:fragment>

      <form class="grid grid-cols-1 md:grid-cols-2 gap-4" on:submit|preventDefault={save}>
        <label class="flex flex-col gap-1">
          <span class="font-mono text-[9px] uppercase tracking-widest opacity-40">name</span>
          <input
            type="text"
            bind:value={form.name}
            placeholder="nightly-quality-sweep"
            class="bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-sm font-mono py-1 placeholder:opacity-30 transition-colors"
          />
        </label>
        <label class="flex flex-col gap-1">
          <span class="font-mono text-[9px] uppercase tracking-widest opacity-40">cron</span>
          <input
            type="text"
            bind:value={form.cron}
            placeholder="0 0 3 * * *"
            class="bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-sm font-mono py-1 placeholder:opacity-30 transition-colors"
          />
        </label>
        <label class="flex flex-col gap-1 md:col-span-2">
          <span class="font-mono text-[9px] uppercase tracking-widest opacity-40">goal</span>
          <input
            type="text"
            bind:value={form.goal}
            placeholder="run the quality gates and fix anything red"
            class="bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-sm font-mono py-1 placeholder:opacity-30 transition-colors"
          />
        </label>
        <label class="flex flex-col gap-1">
          <span class="font-mono text-[9px] uppercase tracking-widest opacity-40">repo</span>
          <input
            type="text"
            bind:value={form.repo}
            placeholder="."
            class="bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-sm font-mono py-1 placeholder:opacity-30 transition-colors"
          />
        </label>
        <label class="flex flex-col gap-1">
          <span class="font-mono text-[9px] uppercase tracking-widest opacity-40">priority</span>
          <select
            bind:value={form.priority}
            class="bg-konjo-deep border border-white/10 rounded px-2 py-1.5 font-mono text-xs outline-none focus:border-konjo-accent"
          >
            <option value="low">low</option>
            <option value="normal">normal</option>
            <option value="high">high</option>
            <option value="critical">critical</option>
          </select>
        </label>
        <div class="md:col-span-2 flex items-center gap-3">
          <button
            type="submit"
            disabled={saving || !form.name.trim() || !form.goal.trim()}
            class="px-4 py-1.5 rounded font-mono text-xs uppercase tracking-widest bg-konjo-accent/10 text-konjo-accent border border-konjo-accent/40 hover:bg-konjo-accent/20 disabled:opacity-30 transition-colors"
          >
            {saving ? 'saving…' : editingId ? 'update' : 'create'}
          </button>
          <label class="flex items-center gap-2 font-mono text-xs opacity-70 cursor-pointer">
            <input type="checkbox" bind:checked={form.enabled} class="accent-[var(--konjo-accent)]" />
            enabled
          </label>
        </div>
      </form>
    </Panel>
  {/if}
</div>
