<script lang="ts">
  import Forge from '$lib/forge/Forge.svelte';
  import LaunchControls from '$lib/components/LaunchControls.svelte';
  import { logs, postTask, cancelTask, stimulate, permissionWaiting, PHASE_COLORS, type AgentState, type TaskOptions } from '$lib/stores/agents';
  import { launchControls } from '$lib/stores/controls';

  export let agent: AgentState | null = null;
  export let onClose: (() => void) | null = null;

  /** Snapshot the shared launch controls into a TaskOptions payload. */
  function options(): TaskOptions {
    const c = $launchControls;
    return {
      priority: c.priority as TaskOptions['priority'],
      model: c.model,
      effort: c.effort,
      branch: c.branch || undefined
    };
  }

  let commandInput = '';
  let isSubmitting = false;
  let submitError = '';

  $: phaseColor = agent ? PHASE_COLORS[agent.phase] ?? '#00d4ff' : '#00d4ff';
  $: agentLogs = agent ? $logs.filter((l) => l.taskId === agent.id).slice(-3) : [];
  $: isWaiting = agent ? $permissionWaiting.has(agent.id) : false;
  $: isRunning = agent?.status === 'running' || agent?.status === 'queued';

  async function handleSubmitCommand() {
    if (!commandInput.trim()) return;
    if (isSubmitting) return;
    isSubmitting = true;
    submitError = '';
    // React immediately — the orb shakes + glows orange the moment the
    // request leaves the pane, not after the server acknowledges it.
    if (agent) stimulate(agent.id);
    try {
      // Existing agent keeps its repo; an empty pane uses the selector's repo.
      const repo = agent?.repo || $launchControls.repo || '';
      await postTask(commandInput.trim(), repo, options());
      commandInput = '';
    } catch (err) {
      console.error('[lopi] postTask failed:', err);
      submitError = err instanceof Error ? err.message : 'failed to submit';
    } finally {
      isSubmitting = false;
    }
  }

  async function handleStop() {
    if (!agent) return;
    await cancelTask(agent.id);
  }

  async function handleRetry() {
    if (!agent || isSubmitting) return;
    isSubmitting = true;
    submitError = '';
    stimulate(agent.id);
    try {
      await postTask(agent.goal, agent.repo, options());
    } catch (err) {
      submitError = err instanceof Error ? err.message : 'retry failed';
    } finally {
      isSubmitting = false;
    }
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

</script>

<!--
  Agent pane — 2-column layout: left content + right sidebar (phase/controls).
  Reordering is driven by the grid's drag-handle overlay; resizing by its
  gutters. The pane fills 100% of its tile.
-->
<div
  class="agent-pane group h-full w-full relative border border-white/10 rounded-lg bg-konjo-deep/60 backdrop-blur-sm flex overflow-hidden"
  class:pane-live={agent && isRunning}
  style:--pane-phase={phaseColor}
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
          <div
            class={`status-dot w-2 h-2 rounded-full flex-shrink-0 ${getStatusColor(agent.status)}`}
            style:--dot-glow={isRunning ? phaseColor : 'transparent'}
          ></div>
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
      <!-- Phase-tinted aura pooled behind the orb; intensifies while live. -->
      {#if agent}
        <div
          class="orb-aura"
          class:orb-aura-live={isRunning}
          style:background={`radial-gradient(circle, ${phaseColor}22 0%, transparent 65%)`}
        ></div>
      {/if}
      <!-- Orb (interactive) -->
      <div class="relative">
        {#if agent}
          <Forge
            pressure={agent.pressure}
            phaseColor={phaseColor}
            activity={agent.activity}
            health={agent.health}
            stimulus={agent.stimulus}
            stimulusKind={agent.stimulusKind}
            size={140}
          />
        {:else}
          <!-- Empty slot placeholder: a calm idle beacon — concentric breathing
               rings + a slowly orbiting spark, all following the theme accent. -->
          <div class="idle-beacon">
            <div class="idle-ring idle-ring-1"></div>
            <div class="idle-ring idle-ring-2"></div>
            <div class="idle-core"></div>
            <div class="idle-orbit"><span class="idle-spark"></span></div>
          </div>
        {/if}
      </div>

      <!-- Idle launcher: pick model/effort/repo/branch, then type a goal below -->
      {#if !agent}
        <div class="mt-5 w-full max-w-sm px-2">
          <LaunchControls dense />
        </div>
      {/if}
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
    <!-- Close button — closes the PANE only. The session stays alive and
         drops into the sidebar (where it can be reopened or permanently
         deleted). This is the close/delete split that fixes resurrecting
         sessions on reload. -->
    {#if onClose}
      <button
        type="button"
        on:click={onClose}
        class="w-5 h-5 flex items-center justify-center bg-white/10 hover:bg-white/25 text-white/60 hover:text-white rounded-full text-[10px] font-bold transition-colors flex-shrink-0"
        title={agent ? 'Close pane (session stays in sidebar)' : 'Close pane'}
        aria-label={agent ? 'Close pane; session stays in sidebar' : 'Close pane'}
      >
        ✕
      </button>
    {/if}

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
          class="press w-12 h-12 text-konjo-sun hover:bg-konjo-sun/10 font-mono text-xl rounded border border-white/10 hover:border-konjo-sun/50 transition-colors flex items-center justify-center"
        >
          ↺
        </button>

        <!-- Stop button (bottom) -->
        <button
          type="button"
          on:click={handleStop}
          disabled={!isRunning}
          title="Stop / Cancel"
          class="press w-12 h-12 text-konjo-rose hover:bg-konjo-rose/10 disabled:opacity-20 disabled:active:scale-100 font-mono text-xl rounded border border-white/10 hover:border-konjo-rose/50 transition-colors flex items-center justify-center"
        >
          ■
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  /* Resting elevation + a hairline accent lift on hover. The pane reads as a
     physical card floating in the void rather than a flat rectangle. */
  .agent-pane {
    box-shadow: var(--shadow-pane);
    transition:
      box-shadow var(--dur-base) var(--ease-out-expo),
      border-color var(--dur-base) var(--ease-out-expo),
      transform var(--dur-base) var(--ease-out-expo);
  }
  .agent-pane:hover {
    border-color: rgba(255, 255, 255, 0.16);
  }
  /* A live pane breathes a faint phase-tinted rim so a busy grid telegraphs
     which agents are actually working at a glance. */
  .pane-live {
    box-shadow:
      var(--shadow-pane),
      inset 0 0 0 1px color-mix(in srgb, var(--pane-phase) 18%, transparent),
      0 0 28px -10px color-mix(in srgb, var(--pane-phase) 50%, transparent);
  }

  /* Aura pooled behind the orb. Soft, slow, never distracting. */
  .orb-aura {
    position: absolute;
    width: 220px;
    height: 220px;
    border-radius: 50%;
    filter: blur(8px);
    opacity: 0.5;
    pointer-events: none;
    transition: opacity var(--dur-slow) var(--ease-out-expo);
  }
  .orb-aura-live {
    opacity: 0.9;
    animation: aura-breathe 4.5s var(--ease-in-out-soft) infinite;
  }
  @keyframes aura-breathe {
    0%,
    100% {
      transform: scale(0.92);
      opacity: 0.7;
    }
    50% {
      transform: scale(1.08);
      opacity: 1;
    }
  }

  /* Status dot gets a soft halo while the agent is live. */
  .status-dot {
    box-shadow: 0 0 0 0 var(--dot-glow);
    transition: box-shadow var(--dur-base) var(--ease-out-expo);
  }
  .pane-live .status-dot {
    box-shadow: 0 0 8px 1px var(--dot-glow);
  }

  /* ── Idle beacon ──────────────────────────────────────────────────────────
     An empty slot still feels alive: two breathing rings, a soft core, and a
     spark that orbits once every few seconds. Accent-aware, motion-safe. */
  .idle-beacon {
    position: relative;
    width: 96px;
    height: 96px;
    display: grid;
    place-items: center;
  }
  .idle-ring {
    position: absolute;
    inset: 0;
    border-radius: 50%;
    border: 1px solid rgb(var(--konjo-accent-rgb) / 0.22);
  }
  .idle-ring-1 {
    animation: idle-pulse 3.2s var(--ease-in-out-soft) infinite;
  }
  .idle-ring-2 {
    inset: 16px;
    border-color: rgb(var(--konjo-accent-rgb) / 0.14);
    animation: idle-pulse 3.2s var(--ease-in-out-soft) infinite 1.6s;
  }
  .idle-core {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: rgb(var(--konjo-accent-rgb) / 0.7);
    box-shadow: 0 0 14px 2px rgb(var(--konjo-accent-rgb) / 0.4);
    animation: idle-core 3.2s var(--ease-in-out-soft) infinite;
  }
  .idle-orbit {
    position: absolute;
    inset: 0;
    animation: idle-spin 6s linear infinite;
  }
  .idle-spark {
    position: absolute;
    top: -2px;
    left: 50%;
    width: 4px;
    height: 4px;
    margin-left: -2px;
    border-radius: 50%;
    background: rgb(var(--konjo-accent-rgb) / 0.9);
    box-shadow: 0 0 8px 1px rgb(var(--konjo-accent-rgb) / 0.6);
  }
  @keyframes idle-pulse {
    0%,
    100% {
      transform: scale(0.94);
      opacity: 0.5;
    }
    50% {
      transform: scale(1.04);
      opacity: 1;
    }
  }
  @keyframes idle-core {
    0%,
    100% {
      opacity: 0.6;
    }
    50% {
      opacity: 1;
    }
  }
  @keyframes idle-spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
