<script lang="ts">
  import { onMount } from 'svelte';
  import {
    listTasks,
    deleteTask,
    createTask,
    taskLogs,
    listDlq,
    retryDlq,
    deleteDlq,
    type TaskRow,
    type DeadLetterRow,
    type LogRow
  } from '$lib/api';
  import Panel from '$lib/components/ui/Panel.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { statusColor, statusLabel, levelColor } from '$lib/components/ui/badges';

  let tasks: TaskRow[] = [];
  let dlq: DeadLetterRow[] = [];
  let loadError = '';
  let loading = true;

  // Detail drawer
  let selected: TaskRow | null = null;
  let selectedLogs: LogRow[] = [];
  let logsLoading = false;

  // Quick-launch form
  let goal = '';
  let repo = '';
  let priority = 'normal';
  let launching = false;
  let launchError = '';

  let refreshTimer: ReturnType<typeof setInterval> | null = null;

  async function refresh() {
    try {
      const [t, d] = await Promise.all([listTasks(), listDlq()]);
      tasks = t.tasks;
      dlq = d.dead_letters;
      loadError = '';
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'failed to load';
    } finally {
      loading = false;
    }
  }

  async function openDetail(t: TaskRow) {
    selected = t;
    logsLoading = true;
    selectedLogs = [];
    try {
      const r = await taskLogs(t.id);
      selectedLogs = r.logs;
    } catch {
      selectedLogs = [];
    } finally {
      logsLoading = false;
    }
  }

  async function launch() {
    if (!goal.trim() || launching) return;
    launching = true;
    launchError = '';
    try {
      await createTask(goal.trim(), repo.trim(), priority);
      goal = '';
      await refresh();
    } catch (e) {
      launchError = e instanceof Error ? e.message : 'launch failed';
    } finally {
      launching = false;
    }
  }

  async function cancel(t: TaskRow) {
    try {
      await deleteTask(t.id);
      await refresh();
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'cancel failed';
    }
  }

  async function retryDead(row: DeadLetterRow) {
    try {
      await retryDlq(row.id);
      await refresh();
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'retry failed';
    }
  }

  async function deleteDead(row: DeadLetterRow) {
    try {
      await deleteDlq(row.id);
      await refresh();
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'delete failed';
    }
  }

  function fmtTime(iso: string | null): string {
    if (!iso) return '—';
    const d = new Date(iso);
    return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
  }

  function isLive(t: TaskRow): boolean {
    const label = statusLabel(t.status).toLowerCase();
    return !['success', 'failed', 'rolledback'].some((s) => label.includes(s));
  }

  onMount(() => {
    refresh();
    refreshTimer = setInterval(refresh, 5000);
    return () => {
      if (refreshTimer) clearInterval(refreshTimer);
    };
  });
</script>

<svelte:head><title>lopi · tasks</title></svelte:head>

<div class="max-w-6xl mx-auto px-6 py-8 space-y-6">
  <!-- Quick launch -->
  <Panel title="Launch" subtitle="queue a new agent run">
    <form class="flex flex-wrap gap-3 items-center" on:submit|preventDefault={launch}>
      <span class="text-konjo-jade opacity-60 font-mono text-sm">></span>
      <input
        type="text"
        bind:value={goal}
        placeholder="goal — what should the agent do?"
        class="flex-1 min-w-48 bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-sm font-mono placeholder:opacity-30 py-1 transition-colors"
      />
      <input
        type="text"
        bind:value={repo}
        placeholder="repo path"
        class="w-44 bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-sm font-mono placeholder:opacity-30 py-1 transition-colors"
      />
      <select
        bind:value={priority}
        class="bg-konjo-deep border border-white/10 rounded px-2 py-1 font-mono text-xs outline-none focus:border-konjo-accent"
      >
        <option value="low">low</option>
        <option value="normal">normal</option>
        <option value="high">high</option>
      </select>
      <button
        type="submit"
        disabled={launching || !goal.trim()}
        class="px-4 py-1.5 rounded font-mono text-xs uppercase tracking-widest bg-konjo-accent/10 text-konjo-accent border border-konjo-accent/40 hover:bg-konjo-accent/20 disabled:opacity-30 transition-colors"
      >
        {launching ? 'launching…' : 'launch'}
      </button>
    </form>
    {#if launchError}
      <p class="mt-2 font-mono text-xs" style:color="var(--konjo-rose)">{launchError}</p>
    {/if}
  </Panel>

  <!-- Task history -->
  <Panel title="Sessions" subtitle="{tasks.length} task{tasks.length === 1 ? '' : 's'} · refreshes every 5s">
    <svelte:fragment slot="actions">
      <button
        type="button"
        on:click={refresh}
        class="font-mono text-[10px] uppercase tracking-widest opacity-50 hover:opacity-100 hover:text-konjo-accent transition-all px-2 py-1"
      >
        refresh
      </button>
    </svelte:fragment>

    {#if loading}
      <EmptyState title="loading…" />
    {:else if loadError}
      <EmptyState error title="backend unreachable" detail={loadError} />
    {:else if tasks.length === 0}
      <EmptyState title="no tasks yet" detail="launch one above to begin" />
    {:else}
      <div class="overflow-x-auto -mx-4 -my-4">
        <table class="w-full text-left font-mono text-xs">
          <thead>
            <tr class="border-b border-white/5 text-[9px] uppercase tracking-widest opacity-40">
              <th class="px-4 py-2 font-normal">status</th>
              <th class="px-4 py-2 font-normal">goal</th>
              <th class="px-4 py-2 font-normal">created</th>
              <th class="px-4 py-2 font-normal">completed</th>
              <th class="px-4 py-2 font-normal text-right">actions</th>
            </tr>
          </thead>
          <tbody>
            {#each tasks as t (t.id)}
              {@const label = statusLabel(t.status)}
              <tr
                class="border-b border-white/5 hover:bg-white/5 cursor-pointer transition-colors"
                class:bg-konjo-accent={selected?.id === t.id}
                class:bg-opacity-5={selected?.id === t.id}
                on:click={() => openDetail(t)}
              >
                <td class="px-4 py-2.5 whitespace-nowrap">
                  <span
                    class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full border text-[10px]"
                    style:color={statusColor(label)}
                    style:border-color={statusColor(label)}
                  >
                    {#if isLive(t)}
                      <span
                        class="w-1 h-1 rounded-full animate-pulse"
                        style:background={statusColor(label)}
                      ></span>
                    {/if}
                    {label}
                  </span>
                </td>
                <td class="px-4 py-2.5 max-w-md truncate">{t.goal}</td>
                <td class="px-4 py-2.5 opacity-50 whitespace-nowrap">{fmtTime(t.created_at)}</td>
                <td class="px-4 py-2.5 opacity-50 whitespace-nowrap">{fmtTime(t.completed_at)}</td>
                <td class="px-4 py-2.5 text-right whitespace-nowrap">
                  {#if isLive(t)}
                    <button
                      type="button"
                      on:click|stopPropagation={() => cancel(t)}
                      class="px-2 py-0.5 rounded border border-white/10 text-konjo-rose hover:border-konjo-rose/50 hover:bg-konjo-rose/10 transition-colors text-[10px] uppercase tracking-widest"
                    >
                      cancel
                    </button>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </Panel>

  <!-- Detail drawer -->
  {#if selected}
    <Panel title={selected.goal} subtitle="task {selected.id.slice(0, 8)} · log tail">
      <svelte:fragment slot="actions">
        <button
          type="button"
          on:click={() => (selected = null)}
          class="w-5 h-5 flex items-center justify-center bg-white/10 hover:bg-white/25 text-white/60 hover:text-white rounded-full text-[10px] font-bold transition-colors"
          aria-label="Close detail"
        >
          ✕
        </button>
      </svelte:fragment>

      {#if logsLoading}
        <EmptyState title="loading logs…" />
      {:else if selectedLogs.length === 0}
        <EmptyState title="no logs recorded" detail="this task has no persisted output" />
      {:else}
        <div class="bg-black/40 rounded p-3 max-h-80 overflow-y-auto font-mono text-[11px] space-y-0.5">
          {#each selectedLogs as log (log.id)}
            <div class="flex gap-2">
              <span class="opacity-30 flex-shrink-0 tabular-nums">
                {new Date(log.ts).toLocaleTimeString()}
              </span>
              <span class="flex-shrink-0 w-10 uppercase" style:color={levelColor(log.level)}>
                {log.level}
              </span>
              <span class="break-words opacity-80">{log.line}</span>
            </div>
          {/each}
        </div>
      {/if}
    </Panel>
  {/if}

  <!-- Dead-letter queue -->
  <Panel
    title="Dead Letters"
    subtitle="{dlq.length} task{dlq.length === 1 ? '' : 's'} exhausted all retries"
  >
    {#if dlq.length === 0}
      <EmptyState title="dead-letter queue is empty" detail="nothing has burned out" />
    {:else}
      <div class="space-y-2">
        {#each dlq as row (row.id)}
          <div
            class="flex items-start gap-3 rounded border border-konjo-rose/20 bg-konjo-rose/5 px-3 py-2.5"
          >
            <div class="flex-1 min-w-0">
              <div class="font-mono text-xs truncate">{row.goal}</div>
              <div class="font-mono text-[10px] opacity-50 mt-0.5 truncate">
                {row.total_attempts} attempts · died {fmtTime(row.dead_at)} · {row.last_error}
              </div>
            </div>
            <div class="flex gap-2 flex-shrink-0">
              <button
                type="button"
                on:click={() => retryDead(row)}
                class="px-2 py-1 rounded border border-white/10 text-konjo-sun hover:border-konjo-sun/50 hover:bg-konjo-sun/10 transition-colors font-mono text-[10px] uppercase tracking-widest"
              >
                ↺ retry
              </button>
              <button
                type="button"
                on:click={() => deleteDead(row)}
                class="px-2 py-1 rounded border border-white/10 text-konjo-rose hover:border-konjo-rose/50 hover:bg-konjo-rose/10 transition-colors font-mono text-[10px] uppercase tracking-widest"
              >
                discard
              </button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </Panel>
</div>
