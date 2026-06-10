<script lang="ts">
  import { onMount } from 'svelte';
  import Forge from '$lib/forge/Forge.svelte';
  import {
    logs,
    postTask,
    cancelTask,
    listRepos,
    listBranches,
    permissionWaiting,
    PHASE_COLORS,
    type AgentState,
    type RepoInfo
  } from '$lib/stores/agents';

  export let agent: AgentState | null = null;
  export let slotIndex: number = 0;
  export let onClose: (() => void) | null = null;
  /** Orb canvas diameter in px — controlled by the parent grid based on slot count. */
  export let orbSize: number = 200;

  let commandInput = '';
  let isSubmitting = false;
  let submitError = '';

  // Per-pane selectors. Defaults match the user's expressed preference:
  // sonnet 4.6 selected, medium effort. `auto` model is always an option.
  let repoChoice = '';
  let baseBranch = '';
  let modelChoice: string = 'claude-sonnet-4-6';
  let effortChoice: 'low' | 'medium' | 'high' | 'max' = 'medium';

  const MODEL_OPTIONS: Array<{ id: string; label: string }> = [
    { id: 'auto', label: 'auto' },
    { id: 'claude-haiku-4-5-20251001', label: 'haiku 4.5' },
    { id: 'claude-sonnet-4-5', label: 'sonnet 4.5' },
    { id: 'claude-sonnet-4-6', label: 'sonnet 4.6' },
    { id: 'claude-opus-4-6', label: 'opus 4.6' },
    { id: 'claude-opus-4-7', label: 'opus 4.7' },
    { id: 'claude-opus-4-8', label: 'opus 4.8' }
  ];
  const EFFORT_OPTIONS: Array<{ id: 'low' | 'medium' | 'high' | 'max'; label: string }> = [
    { id: 'low', label: 'low' },
    { id: 'medium', label: 'med' },
    { id: 'high', label: 'high' },
    { id: 'max', label: 'max' }
  ];

  let repos: RepoInfo[] = [];
  let branches: string[] = [];
  let branchesLoading = false;
  let logScroller: HTMLDivElement | null = null;

  onMount(() => {
    void listRepos().then((r) => (repos = r));
    // Sessions sidebar reopens a past goal by dispatching `lopi:prefill-slot`
    // targeting a specific slot — we honour it locally.
    const onPrefill = (ev: Event) => {
      const detail = (ev as CustomEvent<{ slotIdx: number; goal: string }>).detail;
      if (detail?.slotIdx !== slotIndex) return;
      commandInput = detail.goal;
    };
    window.addEventListener('lopi:prefill-slot', onPrefill);
    return () => window.removeEventListener('lopi:prefill-slot', onPrefill);
  });

  async function refreshBranches(path: string) {
    if (!path) {
      branches = [];
      return;
    }
    branchesLoading = true;
    try {
      branches = await listBranches(path);
    } finally {
      branchesLoading = false;
    }
  }

  // Branch list reflects the chosen repo. Disabled state in the dropdown.
  $: void refreshBranches(repoChoice);

  // Lock the repo dropdown to whatever the live agent is actually running in
  // once it starts — the backend's `task_started` event echoes the resolved
  // path so the UI never lies about where the work is happening.
  $: if (agent && agent.repo && repoChoice !== agent.repo) {
    repoChoice = agent.repo;
  }

  function currentRepo(): string {
    return (repoChoice || agent?.repo || '').trim();
  }

  // Auto-pin the log rail to the latest line when new entries arrive — but
  // only if the user hasn't scrolled up to read earlier output (within 80px
  // of the bottom counts as "still pinned").
  $: if (logScroller && agent && agentLogs.length) {
    const el = logScroller;
    const pinned = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
    if (pinned) {
      queueMicrotask(() => {
        el.scrollTop = el.scrollHeight;
      });
    }
  }

  // While actively running, the orb shifts to a per-phase shade of orange
  // so the pane reads "working" at a glance. Idle / completed / failed
  // states keep the original PHASE_COLORS palette.
  const WORKING_PHASE_COLORS: Record<string, string> = {
    Boot: '#ffb366',
    Discovery: '#ff9933',
    Planning: '#ff7722',
    Implementation: '#ff4500',
    Testing: '#ff8800',
    Conclusion: '#ffa64d'
  };
  $: isWorking = agent?.status === 'running';
  $: phaseColor = agent
    ? (isWorking
        ? WORKING_PHASE_COLORS[agent.phase] ?? '#ff7722'
        : PHASE_COLORS[agent.phase] ?? '#00d4ff')
    : '#00d4ff';
  // Keep the last 200 lines so the user can scroll back through what the
  // agent has done; the rail is scrollable so older lines aren't lost.
  $: agentLogs = agent ? $logs.filter((l) => l.taskId === agent.id).slice(-200) : [];
  $: isWaiting = agent ? $permissionWaiting.has(agent.id) : false;
  $: isRunning = agent?.status === 'running' || agent?.status === 'queued';

  async function handleSubmitCommand() {
    if (!commandInput.trim()) return;
    if (isSubmitting) return;
    isSubmitting = true;
    submitError = '';
    try {
      await postTask(commandInput.trim(), currentRepo(), 'normal', {
        base_branch: baseBranch,
        model: modelChoice,
        effort: effortChoice
      });
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
    try {
      await postTask(agent.goal, agent.repo, 'normal', {
        base_branch: baseBranch,
        model: modelChoice,
        effort: effortChoice
      });
    } catch (err) {
      submitError = err instanceof Error ? err.message : 'retry failed';
    } finally {
      isSubmitting = false;
    }
  }

  function formatElapsed(ms: number): string {
    const s = Math.floor(ms / 1000);
    const m = Math.floor(s / 60);
    const sec = (s % 60).toString().padStart(2, '0');
    if (m > 99) return `${m}m`;
    return `${m.toString().padStart(2, '0')}:${sec}`;
  }

  function formatTokens(t: number): string {
    if (!t) return '0';
    if (t < 1000) return `${t}`;
    if (t < 1_000_000) return `${(t / 1000).toFixed(t < 10_000 ? 2 : 1)}k`;
    return `${(t / 1_000_000).toFixed(2)}M`;
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
  <!-- ── LEFT COLUMN (main content — 2/3 of pane width) ─────────────────── -->
  <div class="h-full flex flex-col flex-[2] min-w-0 overflow-hidden">
    <!-- HEADER ─────────────────────────────────────────────────────── -->
    <div
      class="px-4 py-3 border-b border-white/5 flex items-center justify-between flex-shrink-0 cursor-grab active:cursor-grabbing hover:bg-white/5 transition-colors"
      style:border-color={agent && isRunning ? phaseColor + '40' : 'rgba(255,255,255,0.05)'}
    >
      <div class="flex items-center gap-2 min-w-0 flex-1">
        {#if agent}
          <div class={`w-2.5 h-2.5 rounded-full flex-shrink-0 ${getStatusColor(agent.status)}`}></div>
          <div class="min-w-0 flex-1">
            <div class="font-mono text-sm font-medium leading-tight text-konjo-paper truncate">
              {agent.goal}
            </div>
            <div class="font-mono text-[10px] uppercase tracking-widest opacity-50 mt-0.5">
              {agent.repo}
            </div>
          </div>
        {:else}
          <div class="text-konjo-ice opacity-60 font-mono text-sm">— idle —</div>
        {/if}
      </div>
    </div>

    <!-- ORB AREA (flex-1) ───────────────────────────────────────────── -->
    <div class="flex-1 flex flex-col items-center justify-center relative px-2 py-4 min-h-0">
      <div class="relative">
        {#if agent}
          <Forge
            pressure={isWorking ? agent.pressure : 0}
            phaseColor={phaseColor}
            activity={isWorking ? agent.activity : 0}
            health={agent.health}
            size={orbSize}
          />
        {:else}
          <!-- Empty slot placeholder: pulsing ring sized to match the orb. -->
          <div
            class="rounded-full border-2 border-konjo-ice/20 animate-pulse"
            style:width={`${Math.round(orbSize * 0.7)}px`}
            style:height={`${Math.round(orbSize * 0.7)}px`}
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

        <!-- Combined working indicator: spinner + tokens + timer. Replaces
             the cost display (claude code subscription has no per-call $).
             Tints orange + spins while status === 'running'; static and
             muted otherwise. -->
        <div
          class="lopi-work-pill flex items-center gap-2 px-2 py-1 rounded-md border tabular-nums flex-shrink-0"
          class:lopi-work-pill--running={isWorking}
          title="Tokens used · session time"
        >
          {#if isWorking}
            <svg
              class="lopi-spinner flex-shrink-0"
              width="11"
              height="11"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="3"
              stroke-linecap="round"
              aria-label="working"
            >
              <path d="M21 12a9 9 0 1 1-6.219-8.56" />
            </svg>
          {:else}
            <span
              class="w-[11px] h-[11px] rounded-full opacity-40 flex-shrink-0"
              style:background="currentColor"
            ></span>
          {/if}
          <span class="font-semibold">{formatTokens(agent.tokens)}</span><span class="opacity-60">tok</span>
          <span class="opacity-40">·</span>
          <span class="font-semibold">{formatElapsed(agent.elapsedMs)}</span>
        </div>
      </div>
    {/if}

    <!-- SELECTORS (repo / base branch / model / effort) ───────────── -->
    <div
      class="px-3 py-2 border-t border-white/5 flex flex-wrap gap-1.5 flex-shrink-0 bg-black/40 text-[11px] font-mono lopi-selectors"
    >
      <select
        bind:value={repoChoice}
        title="Repository"
        disabled={isRunning}
        class="flex-1 min-w-[6rem]"
      >
        <option value="">— default repo —</option>
        {#each repos as r (r.path)}
          <option value={r.path}>{r.name}</option>
        {/each}
        {#if repoChoice && !repos.some((r) => r.path === repoChoice)}
          <option value={repoChoice}>{repoChoice}</option>
        {/if}
      </select>

      <!-- Base branch — auto-detected from repo. Disabled until repo chosen. -->
      <select
        bind:value={baseBranch}
        title={repoChoice ? 'Base branch (HEAD if empty)' : 'Select a repo to load branches'}
        disabled={isRunning || !repoChoice}
        class="w-28"
      >
        <option value="">{branchesLoading ? '…' : 'HEAD'}</option>
        {#each branches as b (b)}
          <option value={b}>{b}</option>
        {/each}
      </select>

      <select bind:value={modelChoice} title="Claude model" disabled={isRunning} class="w-28">
        {#each MODEL_OPTIONS as m (m.id)}
          <option value={m.id}>{m.label}</option>
        {/each}
      </select>

      <select
        bind:value={effortChoice}
        title="Effort — drives retry budget"
        disabled={isRunning}
        class="w-20"
      >
        {#each EFFORT_OPTIONS as e (e.id)}
          <option value={e.id}>{e.label}</option>
        {/each}
      </select>
    </div>

    <!-- COMMAND INPUT ─────────────────────────────────────────────── -->
    <div class="px-3 py-3 border-t border-white/5 flex items-center gap-2 flex-shrink-0 bg-black/10">
      <span class="text-konjo-jade opacity-60 flex-shrink-0 font-mono text-base">></span>
      <input
        type="text"
        bind:value={commandInput}
        on:keydown={(e) => {
          if (e.key === 'Enter') handleSubmitCommand();
        }}
        disabled={isSubmitting}
        placeholder={agent ? 'new goal…' : 'type a goal…'}
        class="flex-1 bg-transparent border-b border-white/10 focus:border-konjo-ice outline-none text-sm font-mono placeholder:opacity-30 disabled:opacity-50 transition-colors py-1.5 leading-relaxed"
      />
      {#if isSubmitting}
        <span class="text-konjo-sun opacity-70 flex-shrink-0 font-mono text-sm">⟳</span>
      {/if}
    </div>
    {#if submitError}
      <div class="px-3 py-1 text-[10px] font-mono text-konjo-rose opacity-90 flex-shrink-0">
        {submitError}
      </div>
    {/if}

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

  <!-- ── RIGHT RAIL (controls + live log — 1/3 of pane width) ──────────── -->
  <div
    class="flex-[1] min-w-0 h-full flex flex-col border-l border-white/10 bg-black/40 overflow-hidden"
  >
    <!-- Top strip: phase label + close. "Boot" is suppressed because it's
         a zero-info default — the label only appears once the runner has
         reported a real phase. -->
    <div
      class="px-3 py-2 border-b border-white/10 flex items-center gap-2 flex-shrink-0 bg-black/30"
    >
      {#if agent && agent.phase !== 'Boot'}
        <div
          class="font-display text-xs font-bold leading-tight tracking-tight truncate flex-1 min-w-0"
          style:color={phaseColor}
          title={agent.phase}
        >
          {agent.phase}
        </div>
      {:else}
        <div class="flex-1"></div>
      {/if}
      {#if onClose}
        <button
          type="button"
          on:click={onClose}
          class="w-7 h-7 flex items-center justify-center bg-white/10 hover:bg-white/25 text-white/70 hover:text-white rounded-full text-xs font-bold transition-colors flex-shrink-0"
          title={agent ? 'Close & delete session' : 'Close pane'}
          aria-label={agent ? 'Close and delete session' : 'Close pane'}
        >
          ✕
        </button>
      {/if}
    </div>

    <!-- Log rail. Every line wraps anywhere — no truncation. White text
         body for readability; level letter stays colored so warn/error
         still pop visually. -->
    <div
      bind:this={logScroller}
      class="flex-1 min-h-0 overflow-y-auto px-4 py-3 text-[12px] font-mono leading-relaxed text-white"
    >
      {#if agent && agentLogs.length > 0}
        <div class="space-y-3">
          {#each agentLogs as log (log.ts + log.taskId)}
            <div class="lopi-log-line">
              <span
                class="font-bold mr-2"
                style:color={log.level === 'error'
                  ? 'var(--konjo-rose)'
                  : log.level === 'warn'
                    ? 'var(--konjo-flame)'
                    : 'var(--konjo-ice)'}
              >
                {log.level[0].toUpperCase()}
              </span><span>{log.message}</span>
            </div>
          {/each}
        </div>
      {:else if agent}
        <div class="text-white/40 italic text-xs">— waiting for output —</div>
      {:else}
        <div class="text-white/30 italic text-xs">log will appear here when a goal runs</div>
      {/if}
    </div>

    <!-- Bottom action strip: wait indicator + retry / stop. Sits under the
         log so the buttons are reachable but never crowd the latest output.
         Only rendered when there's an agent — empty panes stay clean. -->
    {#if agent}
      <div
        class="px-3 py-2 border-t border-white/10 flex items-center gap-2 flex-shrink-0 bg-black/30"
      >
        {#if isWaiting}
          <div
            class="text-[9px] font-mono uppercase tracking-widest px-2 py-1 bg-konjo-sun/20 border border-konjo-sun rounded animate-pulse flex-shrink-0"
            style:color="var(--konjo-sun)"
            title="Agent appears stalled — heuristic"
          >
            ⚠ wait
          </div>
        {/if}
        <div class="flex-1"></div>
        <button
          type="button"
          on:click={handleRetry}
          title="Retry task"
          class="w-9 h-9 text-konjo-sun hover:bg-konjo-sun/10 font-mono text-lg rounded border border-white/10 hover:border-konjo-sun/50 transition-colors flex items-center justify-center flex-shrink-0"
          aria-label="Retry"
        >
          ↺
        </button>
        <button
          type="button"
          on:click={handleStop}
          disabled={!isRunning}
          title="Stop / Cancel"
          class="w-9 h-9 text-konjo-rose hover:bg-konjo-rose/10 disabled:opacity-20 font-mono text-lg rounded border border-white/10 hover:border-konjo-rose/50 transition-colors flex items-center justify-center flex-shrink-0"
          aria-label="Stop"
        >
          ■
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  /* Combined working-indicator pill: spinner + token meter + session timer.
     Muted when the agent isn't actively producing; tinted orange + pulsing
     halo while `status === 'running'` so the pane reads "alive" at a glance. */
  .lopi-work-pill {
    color: rgba(255, 255, 255, 0.55);
    border-color: rgba(255, 255, 255, 0.08);
    background-color: rgba(255, 255, 255, 0.02);
    transition: color 0.3s ease, border-color 0.3s ease, background-color 0.3s ease;
  }
  .lopi-work-pill--running {
    color: #ff7722;
    border-color: rgba(255, 119, 34, 0.5);
    background-color: rgba(255, 119, 34, 0.08);
    animation: lopi-pill-pulse 2.4s ease-in-out infinite;
  }
  @keyframes lopi-pill-pulse {
    0%, 100% { box-shadow: 0 0 0 0 rgba(255, 119, 34, 0); }
    50%      { box-shadow: 0 0 0 3px rgba(255, 119, 34, 0.18); }
  }
  .lopi-spinner {
    animation: lopi-spin 1s linear infinite;
    transform-origin: 50% 50%;
  }
  @keyframes lopi-spin {
    to { transform: rotate(360deg); }
  }

  /* Dark-themed dropdowns: the OS-native popup follows the system theme by
     default and renders white-on-white in dark mode. Setting bg + color on
     the element AND its <option> children keeps the popup legible against
     lopi's konjo-deep canvas. */
  .lopi-selectors :global(select) {
    background-color: rgba(8, 11, 18, 0.85);
    color: var(--konjo-paper, #e6e9f0);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 4px;
    padding: 3px 18px 3px 6px;
    font-family: inherit;
    font-size: inherit;
    appearance: none;
    background-image:
      linear-gradient(45deg, transparent 50%, rgba(255, 255, 255, 0.4) 50%),
      linear-gradient(135deg, rgba(255, 255, 255, 0.4) 50%, transparent 50%);
    background-position: calc(100% - 10px) 50%, calc(100% - 6px) 50%;
    background-size: 4px 4px, 4px 4px;
    background-repeat: no-repeat;
    transition: border-color 0.15s ease;
  }
  .lopi-selectors :global(select:hover:not(:disabled)) {
    border-color: rgba(255, 255, 255, 0.3);
  }
  .lopi-selectors :global(select:focus:not(:disabled)) {
    outline: none;
    border-color: var(--konjo-ice, #00d4ff);
  }
  .lopi-selectors :global(select:disabled) {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .lopi-selectors :global(option) {
    background-color: #0b1018;
    color: var(--konjo-paper, #e6e9f0);
  }

  /* Log rail: every line wraps so output is never cut off. `overflow-wrap:
     anywhere` covers unbroken tokens (URLs, hashes, file paths) that
     plain `word-break: break-word` alone leaves overflowing. */
  .lopi-log-line {
    overflow-wrap: anywhere;
    word-break: break-word;
    white-space: pre-wrap;
  }
</style>
