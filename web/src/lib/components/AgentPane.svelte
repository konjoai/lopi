<script lang="ts">
  import Forge from '$lib/forge/Forge.svelte';
  import { logs, postTask, cancelTask, permissionWaiting, PHASE_COLORS } from '$lib/stores/agents';
  import type { AgentState } from '$lib/types';

  export let agent: AgentState | null = null;
  export let slotIndex: number = 0;

  let commandInput = '';
  let isSubmitting = false;

  $: phaseColor = agent ? PHASE_COLORS[agent.phase] ?? '#00d4ff' : '#00d4ff';
  $: agentLogs = agent ? $logs.filter((l) => l.taskId === agent.id).slice(-3) : [];
  $: isWaiting = agent ? $permissionWaiting.has(agent.id) : false;
  $: isRunning = agent?.status === 'running' || agent?.status === 'queued';

  async function handleSubmitCommand() {
    if (!commandInput.trim() || !agent) return;
    isSubmitting = true;
    try {
      await postTask(commandInput.trim(), agent.repo, 'normal');
      commandInput = '';
    } finally {
      isSubmitting = false;
    }
  }

  async function handleStop() {
    if (!agent) return;
    await cancelTask(agent.id);
  }

  function handleRetry() {
    if (!agent) return;
    postTask(agent.goal, agent.repo, 'normal');
  }

  function formatElapsed(ms: number): string {
    const s = Math.floor(ms / 1000);
    const m = Math.floor(s / 60);
    if (m > 0) return `${m}m ${s % 60}s`;
    return `${s}s`;
  }

  function getStatusColor(status: string): string {
    switch (status) {
      case 'running':
        return 'bg-konjo-jade animate-pulse';
      case 'queued':
        return 'bg-konjo-sun';
      case 'completed':
        return 'bg-konjo-jade/50';
      case 'failed':
        return 'bg-konjo-rose';
      case 'cancelled':
        return 'bg-konjo-ice/30';
      default:
        return 'bg-konjo-ice/20';
    }
  }

  function onDragStart(e: DragEvent) {
    e.dataTransfer!.effectAllowed = 'move';
    e.dataTransfer!.setData('text/plain', String(slotIndex));
  }
</script>

<!--
  Agent pane — 2-column layout: left content + right sidebar (phase/controls).
  Draggable by header. Equal-size panes. Fills 100% of container.
-->
<div
  class="h-full w-full relative border border-white/10 rounded-lg bg-konjo-deep/60 backdrop-blur-sm flex overflow-hidden"
  draggable="true"
  on:dragstart={onDragStart}
>
  <!-- ── LEFT COLUMN (main content) ────────────────────────────────────────── -->
  <div class="h-full flex flex-col flex-1 min-w-0 overflow-hidden">
    <!-- HEADER (40px) ─────────────────────────────────────────────────── -->
    <div
      class="px-4 py-3 border-b border-white/5 flex items-center justify-between flex-shrink-0 cursor-grab active:cursor-grabbing hover:bg-white/5 transition-colors"
      style:border-color={agent && isRunning ? phaseColor + '40' : 'rgba(255,255,255,0.05)'}
    >
      <div class="flex items-center gap-2 min-w-0 flex-1">
        {#if agent}
          <div class={`w-2 h-2 rounded-full flex-shrink-0 ${getStatusColor(agent.status)}`}></div>
          <div class="min-w-0 flex-1">
            <div class="font-mono text-xs font-medium leading-tight text-konjo-paper truncate">
              {agent.goal}
            </div>
            <div class="font-mono text-[8px] uppercase tracking-widest opacity-40 mt-0.5">
              {agent.repo}
            </div>
          </div>
        {:else}
          <div class="text-konjo-ice opacity-50 font-mono text-xs">— idle —</div>
        {/if}
      </div>
    </div>

    <!-- ORB AREA (flex-1) ───────────────────────────────────────────── -->
    <div class="flex-1 flex flex-col items-center justify-center relative px-2 py-4 min-h-0">
      <!-- Orb (interactive) -->
      <div class="relative">
        {#if agent}
          <Forge
            pressure={agent.pressure}
            phaseColor={phaseColor}
            activity={agent.activity}
            health={agent.health}
            size={140}
          />
        {:else}
          <!-- Empty slot placeholder: pulsing ring -->
          <div
            class="w-24 h-24 rounded-full border-2 border-konjo-ice/20 animate-pulse"
            style="box-shadow: 0 0 20px rgba(0,212,255,0.1);"
          ></div>
        {/if}
      </div>
    </div>

    <!-- METRICS BAR (40px) ──────────────────────────────────────────── -->
    {#if agent}
      <div class="px-3 py-2 border-t border-white/5 flex items-center justify-between gap-2 text-[9px] font-mono flex-shrink-0 bg-black/20">
        <!-- Token pressure bar -->
        <div class="flex items-center gap-1.5 flex-1 min-w-0">
          <span class="opacity-50 flex-shrink-0">P:</span>
          <div class="h-1.5 flex-1 bg-black/40 rounded-full overflow-hidden">
            <div
              class="h-full rounded-full transition-all duration-300"
              style:width={`${agent.pressure * 100}%`}
              style:background={agent.pressure > 0.75 ? 'var(--konjo-rose)' : 'var(--konjo-ice)'}
            ></div>
          </div>
        </div>

        <!-- Activity -->
        <div class="flex items-center gap-1 flex-shrink-0">
          <span class="opacity-50">A:</span>
          <span class="tabular-nums w-6">{Math.round(agent.activity * 100)}</span>
        </div>

        <!-- Elapsed -->
        <div class="flex items-center gap-1 flex-shrink-0">
          <span class="opacity-50">⏱:</span>
          <span class="tabular-nums w-12">{formatElapsed(agent.elapsedMs)}</span>
        </div>

        <!-- Cost -->
        <div class="flex items-center gap-1 flex-shrink-0" style:color="var(--konjo-flame)">
          <span class="opacity-50">$</span>
          <span class="tabular-nums w-10">{agent.cost.toFixed(4)}</span>
        </div>
      </div>
    {/if}

    <!-- LOG (variable, squeeze-friendly) ────────────────────────────── -->
    {#if agent}
      <div class="px-3 py-2 border-t border-white/5 bg-black/30 text-[8px] font-mono space-y-0.5 flex-shrink-0 overflow-y-auto max-h-12">
        {#if agentLogs.length > 0}
          {#each agentLogs as log (log.ts + log.taskId)}
            <div class="flex gap-1.5 opacity-70">
              <span class="opacity-40 flex-shrink-0">{log.taskId.slice(0, 6)}</span>
              <span
                class="flex-shrink-0"
                style:color={log.level === 'error'
                  ? 'var(--konjo-rose)'
                  : log.level === 'warn'
                    ? 'var(--konjo-flame)'
                    : 'inherit'}
              >
                [{log.level[0].toUpperCase()}]
              </span>
              <span class="break-words truncate">{log.message}</span>
            </div>
          {/each}
        {:else}
          <div class="opacity-30 italic">— waiting for output —</div>
        {/if}
      </div>
    {/if}

    <!-- COMMAND INPUT (40px) ────────────────────────────────────────── -->
    <div class="px-3 py-2 border-t border-white/5 flex gap-2 flex-shrink-0 bg-black/10">
      <span class="text-konjo-jade opacity-60 flex-shrink-0 font-mono text-xs">></span>
      <input
        type="text"
        bind:value={commandInput}
        on:keydown={(e) => {
          if (e.key === 'Enter') handleSubmitCommand();
        }}
        disabled={isSubmitting}
        placeholder={agent ? 'new goal…' : 'type a goal…'}
        class="flex-1 bg-transparent border-b border-white/10 focus:border-konjo-ice outline-none text-xs font-mono placeholder:opacity-30 disabled:opacity-50 transition-colors"
      />
      {#if isSubmitting}
        <span class="text-konjo-sun opacity-70 flex-shrink-0 font-mono text-xs">⟳</span>
      {/if}
    </div>

    <!-- FOOTER: Phase + Attempt ────────────────────────────────────── -->
    {#if agent}
      <div class="px-3 py-1 border-t border-white/5 text-[8px] font-mono opacity-50 flex items-center justify-between flex-shrink-0">
        <span>attempt {agent.attempt}</span>
        {#if agent.branch}
          <span class="truncate">{agent.branch}</span>
        {/if}
      </div>
    {/if}
  </div>

  <!-- ── RIGHT SIDEBAR (phase + awaiting + controls) ────────────────────── -->
  <div
    class="w-20 border-l border-white/5 flex flex-col items-center justify-between py-4 px-2 flex-shrink-0 bg-black/30"
  >
    <!-- Phase display (top) ───────────────────────────────────────── -->
    <div class="flex flex-col items-center gap-2 text-center flex-shrink-0">
      <div
        class="font-display text-sm font-bold leading-tight tracking-tight"
        style:color={phaseColor}
      >
        {agent?.phase ?? '—'}
      </div>

      <!-- Awaiting badge ───────────────────────────────────────────── -->
      {#if agent && isWaiting}
        <div
          class="text-[7px] font-mono uppercase tracking-widest px-1 py-0.5 bg-konjo-sun/20 border border-konjo-sun rounded animate-pulse"
          style:color="var(--konjo-sun)"
        >
          ⚠ wait
        </div>
      {/if}
    </div>

    <!-- Control buttons (bottom) ──────────────────────────────────── -->
    {#if agent}
      <div class="flex flex-col gap-2 flex-shrink-0">
        <!-- Retry button (top) -->
        <button
          type="button"
          on:click={handleRetry}
          title="Retry task"
          class="w-12 h-12 text-konjo-sun hover:bg-konjo-sun/10 font-mono text-xl rounded border border-white/10 hover:border-konjo-sun/50 transition-colors flex items-center justify-center"
        >
          ↺
        </button>

        <!-- Stop button (bottom) -->
        <button
          type="button"
          on:click={handleStop}
          disabled={!isRunning}
          title="Stop / Cancel"
          class="w-12 h-12 text-konjo-rose hover:bg-konjo-rose/10 disabled:opacity-20 font-mono text-xl rounded border border-white/10 hover:border-konjo-rose/50 transition-colors flex items-center justify-center"
        >
          ■
        </button>
      </div>
    {/if}
  </div>
</div>
