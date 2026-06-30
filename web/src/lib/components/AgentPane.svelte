<script lang="ts">
  import ForgeStage from '$lib/components/ForgeStage.svelte';
  import Transcript from '$lib/components/transcript/Transcript.svelte';
  import Composer from '$lib/components/Composer.svelte';
  import LaunchControls from '$lib/components/LaunchControls.svelte';
  import {
    postTask,
    cancelTask,
    stimulate,
    permissionWaiting,
    type AgentState,
    type TaskOptions
  } from '$lib/stores/agents';
  import { transcripts } from '$lib/stores/transcript';
  import { computeOrbState, IDLE_ORB } from '$lib/forge/orbState';
  import { launchControls } from '$lib/stores/controls';
  import { approvePlan, rejectPlan } from '$lib/api';

  export let agent: AgentState | null = null;
  export let onClose: (() => void) | null = null;

  let deciding = false;
  let isSubmitting = false;
  let submitError = '';
  let cornerInset = 0;

  $: isWaiting = agent ? $permissionWaiting.has(agent.id) : false;
  $: isRunning = agent?.status === 'running' || agent?.status === 'queued';
  $: orb = computeOrbState(agent, isWaiting);
  // Drive the pane chrome (header rim, live glow, phase label) from the orb's
  // live state color so the whole pane telegraphs status as one voice.
  $: phaseColor = agent ? orb.glowColor : '#00d4ff';
  $: blocks = agent ? ($transcripts.get(agent.id) ?? []) : [];
  $: streaming = agent?.status === 'running';

  /** Snapshot the shared launch controls into a TaskOptions payload. */
  function options(): TaskOptions {
    const c = $launchControls;
    return { priority: c.priority as TaskOptions['priority'], model: c.model, effort: c.effort, branch: c.branch || undefined };
  }

  async function decidePlan(approve: boolean) {
    if (!agent || deciding) return;
    deciding = true;
    try {
      await (approve ? approvePlan(agent.id) : rejectPlan(agent.id));
    } catch (err) {
      console.error('[lopi] plan decision failed:', err);
    } finally {
      deciding = false;
    }
  }

  async function submit(text: string) {
    if (isSubmitting) return;
    isSubmitting = true;
    submitError = '';
    if (agent) stimulate(agent.id);
    try {
      const repo = agent?.repo || $launchControls.repo || '';
      await postTask(text, repo, options());
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
    return m > 0 ? `${m}m ${s % 60}s` : `${s}s`;
  }

  function statusColor(status: string): string {
    switch (status) {
      case 'running': return 'bg-konjo-jade animate-pulse';
      case 'queued': return 'bg-konjo-sun';
      case 'completed': return 'bg-konjo-jade/50';
      case 'failed': return 'bg-konjo-rose';
      case 'cancelled': return 'bg-konjo-ice/30';
      default: return 'bg-konjo-ice/20';
    }
  }
</script>

<!--
  Agent pane — a full-pane chat: header, a transcript that fills the body, and a
  composer pinned at the bottom. The living orb is absorbed into the bottom-right
  corner of the transcript (text wraps around it); a thin right rail keeps close,
  phase and stop/retry. With no session the orb is the centered launcher.
-->
<div
  class="agent-pane group h-full w-full relative border border-white/10 rounded-lg bg-konjo-deep/60 backdrop-blur-sm flex overflow-hidden"
  class:pane-live={agent && isRunning}
  style:--pane-phase={phaseColor}
>
  <div class="h-full flex flex-col flex-1 min-w-0 overflow-hidden">
    <!-- HEADER ───────────────────────────────────────────────────────────── -->
    <div
      class="px-4 py-3 border-b border-white/5 flex items-center justify-between flex-shrink-0 cursor-grab active:cursor-grabbing hover:bg-white/5 transition-colors"
      style:border-color={agent && isRunning ? phaseColor + '40' : 'rgba(255,255,255,0.05)'}
    >
      <div class="flex items-center gap-2 min-w-0 flex-1">
        {#if agent}
          <div class={`status-dot w-2 h-2 rounded-full flex-shrink-0 ${statusColor(agent.status)}`} style:--dot-glow={isRunning ? phaseColor : 'transparent'}></div>
          <div class="min-w-0 flex-1">
            <div class="font-mono text-xs font-medium leading-tight text-konjo-paper truncate">{agent.goal}</div>
            <div class="font-mono text-[8px] uppercase tracking-widest opacity-40 mt-0.5">{agent.repo}</div>
          </div>
        {:else}
          <div class="text-konjo-ice opacity-50 font-mono text-xs">— idle —</div>
        {/if}
      </div>
    </div>

    <!-- BODY: transcript fills it; the orb floats bottom-right above the composer -->
    <div class="pane-body relative flex-1 min-h-0 overflow-hidden">
      {#if agent}
        <Transcript {blocks} {streaming} orbInset={cornerInset} />
        <ForgeStage live {agent} {orb} onCornerSize={(px) => (cornerInset = px)} />
      {:else}
        <ForgeStage live={false} agent={null} orb={IDLE_ORB}>
          <LaunchControls dense />
        </ForgeStage>
      {/if}

      <!-- Plan approval gate — overlays the body when paused for review. -->
      {#if agent && agent.awaitingApproval}
        <div class="plan-gate absolute inset-2 flex flex-col rounded-lg border border-konjo-sun/40 bg-konjo-deep/95 backdrop-blur-md overflow-hidden z-20">
          <div class="px-3 py-2 border-b border-white/10 flex items-center gap-2 flex-shrink-0">
            <span class="text-konjo-sun text-sm leading-none">⏸</span>
            <span class="font-display text-xs font-bold text-konjo-sun">Plan ready · review</span>
            <span class="ml-auto font-mono text-[8px] uppercase tracking-widest opacity-40">attempt {agent.attempt}</span>
          </div>
          <div class="flex-1 overflow-y-auto px-3 py-2 min-h-0">
            {#if agent.planSteps && agent.planSteps.length > 0}
              <ol class="space-y-1.5">
                {#each agent.planSteps as step, i}
                  <li class="flex gap-2 font-mono text-[10px] leading-snug">
                    <span class="text-konjo-sun/70 flex-shrink-0 tabular-nums">{i + 1}.</span>
                    <span class="opacity-80">{step}</span>
                  </li>
                {/each}
              </ol>
            {:else}
              <pre class="font-mono text-[10px] leading-snug whitespace-pre-wrap opacity-80">{agent.planText ?? '—'}</pre>
            {/if}
          </div>
          <div class="flex gap-2 px-3 py-2 border-t border-white/10 flex-shrink-0">
            <button type="button" on:click={() => decidePlan(true)} disabled={deciding} class="press flex-1 py-1.5 rounded-md bg-konjo-jade/15 border border-konjo-jade/50 text-konjo-jade font-mono text-[11px] uppercase tracking-widest hover:bg-konjo-jade/25 disabled:opacity-40 transition-colors">✓ approve</button>
            <button type="button" on:click={() => decidePlan(false)} disabled={deciding} class="press flex-1 py-1.5 rounded-md bg-konjo-rose/15 border border-konjo-rose/50 text-konjo-rose font-mono text-[11px] uppercase tracking-widest hover:bg-konjo-rose/25 disabled:opacity-40 transition-colors">✕ reject</button>
          </div>
        </div>
      {/if}
    </div>

    <!-- METRICS BAR ──────────────────────────────────────────────────────── -->
    {#if agent}
      <div class="px-3 py-1.5 border-t border-white/5 flex items-center justify-between gap-2 text-[9px] font-mono flex-shrink-0 bg-black/20">
        <div class="flex items-center gap-1.5 flex-1 min-w-0">
          <span class="opacity-50 flex-shrink-0">P:</span>
          <div class="h-1.5 flex-1 bg-black/40 rounded-full overflow-hidden">
            <div class="h-full rounded-full transition-all duration-300" style:width={`${agent.pressure * 100}%`} style:background={agent.pressure > 0.75 ? 'var(--konjo-rose)' : 'var(--konjo-ice)'}></div>
          </div>
        </div>
        <div class="flex items-center gap-1 flex-shrink-0"><span class="opacity-50">A:</span><span class="tabular-nums w-6">{Math.round(agent.activity * 100)}</span></div>
        <div class="flex items-center gap-1 flex-shrink-0"><span class="opacity-50">⏱:</span><span class="tabular-nums w-12">{formatElapsed(agent.elapsedMs)}</span></div>
        <div class="flex items-center gap-1 flex-shrink-0" style:color="var(--konjo-flame)"><span class="opacity-50">$</span><span class="tabular-nums w-10">{agent.cost.toFixed(4)}</span></div>
      </div>
    {/if}

    <!-- COMPOSER ─────────────────────────────────────────────────────────── -->
    <Composer hasAgent={!!agent} {isSubmitting} error={submitError} onSubmit={submit} />

    <!-- FOOTER ───────────────────────────────────────────────────────────── -->
    {#if agent}
      <div class="px-3 py-1 border-t border-white/5 text-[8px] font-mono opacity-50 flex items-center justify-between flex-shrink-0">
        <span>attempt {agent.attempt}</span>
        {#if agent.branch}<span class="truncate">{agent.branch}</span>{/if}
      </div>
    {/if}
  </div>

  <!-- ── RIGHT RAIL (close · phase · retry/stop) ───────────────────────────── -->
  <div class="w-16 border-l border-white/5 flex flex-col items-center justify-between py-4 px-2 flex-shrink-0 bg-black/30">
    {#if onClose}
      <button type="button" on:click={onClose} class="w-5 h-5 flex items-center justify-center bg-white/10 hover:bg-white/25 text-white/60 hover:text-white rounded-full text-[10px] font-bold transition-colors flex-shrink-0" title={agent ? 'Close pane (session stays in sidebar)' : 'Close pane'} aria-label={agent ? 'Close pane; session stays in sidebar' : 'Close pane'}>✕</button>
    {/if}

    <div class="flex flex-col items-center gap-2 text-center flex-shrink-0">
      <div class="font-display text-xs font-bold leading-tight tracking-tight" style:color={phaseColor}>{agent?.phase ?? '—'}</div>
      {#if agent && isWaiting}
        <div class="text-[7px] font-mono uppercase tracking-widest px-1 py-0.5 bg-konjo-sun/20 border border-konjo-sun rounded animate-pulse" style:color="var(--konjo-sun)">⚠ wait</div>
      {/if}
    </div>

    {#if agent}
      <div class="flex flex-col gap-2 flex-shrink-0">
        <button type="button" on:click={handleRetry} title="Retry task" class="press w-10 h-10 text-konjo-sun hover:bg-konjo-sun/10 font-mono text-lg rounded border border-white/10 hover:border-konjo-sun/50 transition-colors flex items-center justify-center">↺</button>
        <button type="button" on:click={handleStop} disabled={!isRunning} title="Stop / Cancel" class="press w-10 h-10 text-konjo-rose hover:bg-konjo-rose/10 disabled:opacity-20 disabled:active:scale-100 font-mono text-lg rounded border border-white/10 hover:border-konjo-rose/50 transition-colors flex items-center justify-center">■</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .agent-pane {
    box-shadow: var(--shadow-pane);
    transition: box-shadow var(--dur-base) var(--ease-out-expo), border-color var(--dur-base) var(--ease-out-expo), transform var(--dur-base) var(--ease-out-expo);
  }
  .agent-pane:hover {
    border-color: rgba(255, 255, 255, 0.16);
  }
  .pane-live {
    box-shadow: var(--shadow-pane), inset 0 0 0 1px color-mix(in srgb, var(--pane-phase) 18%, transparent), 0 0 28px -10px color-mix(in srgb, var(--pane-phase) 50%, transparent);
  }
  .status-dot {
    box-shadow: 0 0 0 0 var(--dot-glow);
    transition: box-shadow var(--dur-base) var(--ease-out-expo);
  }
  .pane-live .status-dot {
    box-shadow: 0 0 8px 1px var(--dot-glow);
  }
</style>
