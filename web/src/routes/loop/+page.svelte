<!--
  Loop Engineering — the cockpit for lopi's autonomous loops.

  Phase 16 of the competitive roadmap. One screen that surfaces every loop
  lever for the primary repo: the effective `.lopi/loop.toml` (with validation),
  the L1–L4 phased-autonomy ladder, each schedule's trust level (the single
  writable control here), the discovered skills + rules, and the Konjo quality
  gates that say "no" to the loop. Read-mostly by design — loop config is
  loop-as-code, edited in the repo; the UI governs trust and observes health.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import {
    getLoopEngineering,
    getLoopHealth,
    getLoopRuns,
    getLoopRunTrace,
    setScheduleAutonomy,
    setLoopStrategy,
    setLoopEscalation,
    type LoopSnapshot,
    type LoopHealth,
    type LoopRun,
    type LoopRunTrace,
    type AutonomyOption,
    type SelfPromptOption
  } from '$lib/api';
  import Panel from '$lib/components/ui/Panel.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Dropdown from '$lib/components/ui/Dropdown.svelte';
  import StatCard from '$lib/components/ui/StatCard.svelte';
  import Sparkline from '$lib/components/ui/Sparkline.svelte';
  import type { Option } from '$lib/stores/controls';

  let snap: LoopSnapshot | null = null;
  let health: LoopHealth | null = null;
  let runs: LoopRun[] = [];
  let loading = true;
  let loadError = '';
  let flash = '';

  // Per-run drill-down state.
  let selectedRun: string | null = null;
  let trace: LoopRunTrace | null = null;
  let traceLoading = false;

  async function refresh() {
    try {
      // Config, health and the run list are independent reads — fetch concurrently.
      const [s, h, r] = await Promise.all([getLoopEngineering(), getLoopHealth(), getLoopRuns()]);
      snap = s;
      health = h;
      runs = r.runs ?? [];
      loadError = '';
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'failed to load';
    } finally {
      loading = false;
    }
  }

  onMount(refresh);

  async function selectRun(id: string) {
    if (selectedRun === id) {
      // Toggle the drill-down closed.
      selectedRun = null;
      trace = null;
      return;
    }
    selectedRun = id;
    trace = null;
    traceLoading = true;
    try {
      trace = await getLoopRunTrace(id);
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'failed to load run trace';
    } finally {
      traceLoading = false;
    }
  }

  // The four loop lifecycle stages, shown per attempt for structure.
  const STAGES = ['plan', 'implement', 'test', 'score'];

  function outcomeBadge(o: string): string {
    return (
      { success: 'text-konjo-jade', retry: 'text-konjo-sun', stalled: 'text-konjo-rose' }[o] ??
      'text-konjo-ice'
    );
  }

  function fmtCost(c: number): string {
    return c >= 0.01 ? `$${c.toFixed(2)}` : `$${c.toFixed(4)}`;
  }

  function fmtTokens(t: number): string {
    return t >= 1000 ? `${(t / 1000).toFixed(1)}k` : `${t}`;
  }

  // ── Loop Health derived series ───────────────────────────────────────────────
  $: scoreSeries = (health?.attempts ?? []).map((a) => a.test_pass_rate);
  $: diffSeries = (health?.attempts ?? []).map((a) => a.diff_lines);
  $: costSeries = (health?.burn ?? []).map((b) => b.cost_usd);
  $: pressureSeries = (health?.burn ?? []).map((b) => b.context_pressure);
  $: outcomeTotal = (health?.outcomes ?? []).reduce((n, o) => n + o.count, 0);

  function pct(x: number): string {
    return `${Math.round(x * 100)}%`;
  }

  // Outcome → accent. success is calm jade; stuck/failed runs heat up.
  function outcomeColor(label: string): string {
    if (label === 'success') return 'var(--konjo-jade)';
    if (label === 'retry') return 'var(--konjo-sun)';
    return 'var(--konjo-rose)';
  }

  // The autonomy ladder, as Dropdown options.
  $: autonomyOptions = (snap?.autonomy_levels ?? []).map(
    (l: AutonomyOption): Option => ({ value: l.value, label: `${l.tag} · ${l.label}`, hint: ladderHint(l) })
  );

  function ladderHint(l: AutonomyOption): string {
    if (l.allows_auto_merge) return 'auto-merge on pass';
    if (l.requires_verifier) return 'verify before PR';
    if (l.opens_pr) return 'draft PR, human approves';
    return 'report only, no PR';
  }

  // Accent per trust level: hotter = more autonomous.
  function levelColor(tag: string): string {
    return (
      { L1: 'text-konjo-ice', L2: 'text-konjo-jade', L3: 'text-konjo-sun', L4: 'text-konjo-ember' }[
        tag
      ] ?? 'text-konjo-accent'
    );
  }

  async function changeAutonomy(id: string, level: string) {
    try {
      await setScheduleAutonomy(id, level);
      flash = 'trust level updated';
      setTimeout(() => (flash = ''), 1800);
      await refresh();
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'update failed';
    }
  }

  // The strategy whose self-prompt preview is currently shown. Defaults to the
  // repo's effective strategy; clicking a card focuses it.
  let focusedStrategy = '';
  $: strategyOptions = (snap?.self_prompt_strategies ?? []).map(
    (st: SelfPromptOption): Option => ({ value: st.value, label: `${st.tag} · ${st.label}` })
  );
  $: activeStrategy = focusedStrategy || snap?.config.self_prompt || 'direct';
  $: previewStrategy =
    (snap?.self_prompt_strategies ?? []).find((st) => st.value === activeStrategy) ?? null;

  async function changeStrategy(strategy: string) {
    try {
      await setLoopStrategy(strategy);
      focusedStrategy = strategy;
      flash = 'self-prompting strategy saved to .lopi/loop.toml';
      setTimeout(() => (flash = ''), 2200);
      await refresh();
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'update failed';
    }
  }

  // Accent per strategy: more scaffolding = warmer.
  function strategyColor(tag: string): string {
    return (
      { S1: 'text-konjo-ice', S2: 'text-konjo-jade', S3: 'text-konjo-sun', S4: 'text-konjo-ember' }[
        tag
      ] ?? 'text-konjo-accent'
    );
  }

  // Border accent for a strategy card: active (saved) > focused (previewing) > idle.
  function strategyBorder(value: string): string {
    if (value === snap?.config.self_prompt) return 'border-konjo-accent';
    if (value === activeStrategy) return 'border-white/30';
    return 'border-white/5';
  }

  async function toggleEscalation(enabled: boolean) {
    try {
      await setLoopEscalation(enabled);
      flash = enabled ? 'escalation on — S1→S4 per attempt' : 'escalation off — strategy pinned';
      setTimeout(() => (flash = ''), 2200);
      await refresh();
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'update failed';
    }
  }

  function fmtBudget(t: number): string {
    return t === 0 ? 'inherit global' : `${t.toLocaleString()} tokens`;
  }
</script>

<svelte:head><title>lopi · loop</title></svelte:head>

<div class="max-w-5xl mx-auto px-6 py-8 space-y-6">
  <!-- Header -->
  <div class="flex items-end justify-between flex-wrap gap-4">
    <div>
      <h1 class="font-display text-2xl">Loop Engineering</h1>
      <p class="font-mono text-[11px] uppercase tracking-widest opacity-50 mt-1">
        loop-as-code · trust levels · guardrails
      </p>
    </div>
    {#if snap}
      <div class="font-mono text-[10px] opacity-50 text-right">
        <div class="uppercase tracking-widest">repo</div>
        <div class="text-konjo-accent truncate max-w-[18rem]">{snap.repo}</div>
      </div>
    {/if}
  </div>

  {#if flash}
    <div class="font-mono text-[11px] text-konjo-jade">✓ {flash}</div>
  {/if}

  {#if loading}
    <div class="font-mono text-sm opacity-50">loading loop config…</div>
  {:else if loadError}
    <EmptyState title="Couldn't load loop engineering" detail={loadError} />
  {:else if snap}
    <!-- Loop Health — the visibility pillar: is the loop actually working? -->
    {#if health}
      <Panel title="Loop Health" subtitle="observe · evaluate · improve">
        <div slot="actions">
          <span class="font-mono text-[10px] opacity-40 uppercase tracking-widest">
            {health.stats.attempts} attempts · {health.stats.runs} runs
          </span>
        </div>

        <div class="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 gap-3">
          <StatCard
            label="Success rate"
            value={pct(health.stats.success_rate)}
            color={health.stats.success_rate >= 0.8
              ? 'var(--konjo-jade)'
              : health.stats.success_rate >= 0.5
                ? 'var(--konjo-sun)'
                : 'var(--konjo-rose)'}
          />
          <StatCard
            label="Verifier pass"
            value={health.stats.verifier_total === 0 ? '—' : pct(health.stats.verifier_pass_rate)}
            color="var(--konjo-ice)"
          />
          <StatCard label="Runs" value={health.stats.runs} />
          <StatCard label="Spend" value={`$${health.stats.spend_usd.toFixed(2)}`} color="var(--konjo-sun)" />
          <StatCard
            label="Tokens"
            value={health.stats.tokens >= 1000
              ? `${(health.stats.tokens / 1000).toFixed(1)}k`
              : health.stats.tokens}
          />
        </div>

        {#if scoreSeries.length > 0}
          <div class="grid grid-cols-1 md:grid-cols-2 gap-5 mt-5">
            <div>
              <div class="flex items-center justify-between mb-1.5">
                <span class="font-mono text-[10px] uppercase tracking-widest opacity-40"
                  >Score / attempt</span
                >
                <span class="font-mono text-[10px] text-konjo-jade"
                  >{pct(scoreSeries[scoreSeries.length - 1])}</span
                >
              </div>
              <Sparkline values={scoreSeries} color="var(--konjo-jade)" min={0} max={1} />
            </div>
            <div>
              <div class="flex items-center justify-between mb-1.5">
                <span class="font-mono text-[10px] uppercase tracking-widest opacity-40"
                  >Context pressure</span
                >
                <span class="font-mono text-[10px] text-konjo-ice"
                  >{pressureSeries.length ? pct(pressureSeries[pressureSeries.length - 1]) : '—'}</span
                >
              </div>
              <Sparkline values={pressureSeries} color="var(--konjo-ice)" min={0} max={1} />
            </div>
            <div>
              <div class="flex items-center justify-between mb-1.5">
                <span class="font-mono text-[10px] uppercase tracking-widest opacity-40"
                  >Diff size / attempt</span
                >
                <span class="font-mono text-[10px] opacity-60"
                  >{diffSeries[diffSeries.length - 1]}L</span
                >
              </div>
              <Sparkline values={diffSeries} color="var(--konjo-accent)" min={null} max={null} />
            </div>
            <div>
              <div class="flex items-center justify-between mb-1.5">
                <span class="font-mono text-[10px] uppercase tracking-widest opacity-40"
                  >Cost burn / turn</span
                >
                <span class="font-mono text-[10px] text-konjo-sun"
                  >${health.stats.spend_usd.toFixed(2)}</span
                >
              </div>
              <Sparkline values={costSeries} color="var(--konjo-sun)" min={null} max={null} />
            </div>
          </div>

          <!-- Outcome distribution -->
          {#if outcomeTotal > 0}
            <div class="mt-5">
              <div class="font-mono text-[10px] uppercase tracking-widest opacity-40 mb-2">
                Outcome distribution
              </div>
              <div class="flex h-2.5 rounded-full overflow-hidden bg-konjo-black/40">
                {#each health.outcomes as o}
                  <div
                    style="width: {(o.count / outcomeTotal) * 100}%; background: {outcomeColor(o.label)}"
                    title="{o.label}: {o.count}"
                  ></div>
                {/each}
              </div>
              <div class="flex flex-wrap gap-x-4 gap-y-1 mt-2">
                {#each health.outcomes as o}
                  <span class="font-mono text-[10px] opacity-60 flex items-center gap-1.5">
                    <span class="w-2 h-2 rounded-full" style="background: {outcomeColor(o.label)}"></span>
                    {o.label} · {o.count}
                  </span>
                {/each}
              </div>
            </div>
          {/if}
        {:else}
          <div class="mt-4">
            <EmptyState title="No loop telemetry yet" detail="Run a loop to populate health metrics." />
          </div>
        {/if}
      </Panel>
    {/if}

    <!-- Recent runs + per-run drill-down trace -->
    <Panel title="Recent Runs" subtitle="click a run for its attempt-by-attempt trace">
      {#if runs.length === 0}
        <EmptyState title="No runs yet" detail="Loop runs appear here once a task executes." />
      {:else}
        <div class="space-y-1.5">
          {#each runs as r (r.task_id)}
            <button
              type="button"
              on:click={() => selectRun(r.task_id)}
              class="w-full text-left flex items-center gap-3 rounded-lg border px-3 py-2.5 transition-colors"
              class:border-konjo-accent={selectedRun === r.task_id}
              class:border-white={selectedRun !== r.task_id}
              class:border-opacity-5={selectedRun !== r.task_id}
              class:bg-konjo-black={true}
              class:bg-opacity-40={true}
            >
              <span class="font-mono text-[10px] opacity-30 w-3 flex-shrink-0"
                >{selectedRun === r.task_id ? '▾' : '▸'}</span
              >
              <div class="min-w-0 flex-1">
                <div class="font-mono text-[12px] truncate">{r.goal}</div>
                <div class="font-mono text-[10px] opacity-40">
                  {r.attempts} attempt{r.attempts === 1 ? '' : 's'} · best {Math.round(
                    r.best_score * 100
                  )}%
                </div>
              </div>
              <span class="font-mono text-[10px] uppercase tracking-widest {outcomeBadge(r.final_outcome)}"
                >{r.final_outcome}</span
              >
            </button>

            {#if selectedRun === r.task_id}
              <div class="ml-3 pl-3 border-l border-white/10 py-2 space-y-2">
                {#if traceLoading}
                  <div class="font-mono text-[11px] opacity-40">loading trace…</div>
                {:else if trace}
                  {#each trace.attempts as a (a.attempt)}
                    <div class="rounded-lg border border-white/5 bg-konjo-deep/50 px-3 py-2.5">
                      <div class="flex items-center justify-between gap-3">
                        <span class="font-display text-[13px]">Attempt {a.attempt}</span>
                        <span
                          class="font-mono text-[10px] uppercase tracking-widest {outcomeBadge(a.outcome)}"
                          >{a.outcome}</span
                        >
                      </div>
                      <!-- lifecycle stages -->
                      <div class="flex items-center gap-1.5 mt-2">
                        {#each STAGES as st, i}
                          <span class="font-mono text-[9px] uppercase tracking-widest opacity-50"
                            >{st}</span
                          >
                          {#if i < STAGES.length - 1}
                            <span class="opacity-20 text-[9px]">→</span>
                          {/if}
                        {/each}
                      </div>
                      <!-- metrics -->
                      <div class="flex flex-wrap gap-x-4 gap-y-1 mt-2 font-mono text-[11px]">
                        <span class="opacity-70">pass <span class="text-konjo-jade">{Math.round(a.test_pass_rate * 100)}%</span></span>
                        <span class="opacity-70">lint <span class:text-konjo-rose={a.lint_errors > 0}>{a.lint_errors}</span></span>
                        <span class="opacity-70">diff {a.diff_lines}L</span>
                        <span class="opacity-70">{fmtTokens(a.tokens)} tok</span>
                        <span class="opacity-70 text-konjo-sun">{fmtCost(a.cost_usd)}</span>
                      </div>
                      <!-- verifier verdict -->
                      {#if a.verifier}
                        <div class="mt-2 font-mono text-[10px]">
                          <span class={a.verifier.passed ? 'text-konjo-jade' : 'text-konjo-rose'}>
                            {a.verifier.passed ? '✓ verifier passed' : '✗ verifier rejected'} ·
                            {Math.round(a.verifier.confidence * 100)}%
                          </span>
                          {#if a.verifier.gaps.length}
                            <ul class="mt-1 space-y-0.5 opacity-60">
                              {#each a.verifier.gaps as g}
                                <li>• {g}</li>
                              {/each}
                            </ul>
                          {/if}
                        </div>
                      {/if}
                      <!-- errors -->
                      {#if a.errors.length}
                        <ul class="mt-2 space-y-0.5 font-mono text-[10px] text-konjo-rose/70">
                          {#each a.errors.slice(0, 4) as err}
                            <li class="truncate">• {err}</li>
                          {/each}
                        </ul>
                      {/if}
                    </div>
                  {/each}
                {/if}
              </div>
            {/if}
          {/each}
        </div>
      {/if}
    </Panel>

    <!-- Effective loop config -->
    <Panel title="Effective Config" subtitle=".lopi/loop.toml">
      <div slot="actions">
        {#if snap.config.valid}
          <span class="font-mono text-[10px] text-konjo-jade uppercase tracking-widest">valid</span>
        {:else}
          <span class="font-mono text-[10px] text-konjo-rose uppercase tracking-widest"
            >{snap.config.issues.length} issue(s)</span
          >
        {/if}
      </div>
      <div class="grid grid-cols-2 sm:grid-cols-3 gap-4 font-mono text-[12px]">
        <div>
          <div class="text-[10px] uppercase tracking-widest opacity-40">Default Autonomy</div>
          <div class="mt-1 {levelColor(snap.config.autonomy_tag)}">
            {snap.config.autonomy_tag} · {snap.config.autonomy_label}
          </div>
        </div>
        <div>
          <div class="text-[10px] uppercase tracking-widest opacity-40">Vision Anchor</div>
          <div class="mt-1 opacity-80">{snap.config.vision_path ?? '—'}</div>
        </div>
        <div>
          <div class="text-[10px] uppercase tracking-widest opacity-40">Per-run Budget</div>
          <div class="mt-1 opacity-80">{fmtBudget(snap.config.budget_tokens)}</div>
        </div>
        <div>
          <div class="text-[10px] uppercase tracking-widest opacity-40">No-progress Halt</div>
          <div class="mt-1 opacity-80">{snap.config.no_progress_limit} iterations</div>
        </div>
        <div>
          <div class="text-[10px] uppercase tracking-widest opacity-40">Max Iterations</div>
          <div class="mt-1 opacity-80">{snap.config.max_iterations}</div>
        </div>
        <div>
          <div class="text-[10px] uppercase tracking-widest opacity-40">Skills / Rules</div>
          <div class="mt-1 opacity-80">
            {snap.config.skills_enabled.length || 'all'} / {snap.config.rules_enabled.length || 'all'}
          </div>
        </div>
      </div>
      {#if !snap.config.valid}
        <ul class="mt-4 space-y-1 font-mono text-[11px] text-konjo-rose">
          {#each snap.config.issues as issue}
            <li>• {issue}</li>
          {/each}
        </ul>
      {/if}
    </Panel>

    <!-- The autonomy ladder -->
    <Panel title="Autonomy Ladder" subtitle="L1 → L4 · trust earned incrementally">
      <div class="grid grid-cols-2 sm:grid-cols-4 gap-3">
        {#each snap.autonomy_levels as l}
          <div class="rounded-lg border border-white/5 bg-konjo-black/40 p-3">
            <div class="font-mono text-sm font-bold {levelColor(l.tag)}">{l.tag}</div>
            <div class="font-display text-[13px] mt-0.5">{l.label}</div>
            <div class="font-mono text-[10px] opacity-50 mt-1.5 leading-relaxed">{ladderHint(l)}</div>
          </div>
        {/each}
      </div>
    </Panel>

    <!-- Self-prompting strategy: how the agent re-prompts itself on retry -->
    <Panel title="Self-Prompting Strategy" subtitle="how the loop re-prompts itself after a failed attempt">
      <div slot="actions" class="w-52">
        <Dropdown
          value={snap.config.self_prompt}
          options={strategyOptions}
          on:change={(e) => changeStrategy(e.detail)}
        />
      </div>
      <p class="font-mono text-[11px] opacity-50 mb-3 leading-relaxed">
        The single highest-leverage loop lever — the text the agent feeds back into its own next
        plan. Picking one writes <span class="text-konjo-accent">.lopi/loop.toml</span> (loop-as-code)
        and the runner honors it live on the next adaptive retry.
      </p>
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
        {#each snap.self_prompt_strategies as st}
          <button
            type="button"
            class="text-left rounded-lg border bg-konjo-black/40 p-3 transition-colors {strategyBorder(
              st.value
            )}"
            on:click={() => (focusedStrategy = st.value)}
          >
            <div class="flex items-center gap-2">
              <span class="font-mono text-sm font-bold {strategyColor(st.tag)}">{st.tag}</span>
              <span class="font-display text-[13px]">{st.label}</span>
              {#if st.value === snap.config.self_prompt}
                <span class="ml-auto font-mono text-[9px] uppercase tracking-widest text-konjo-jade"
                  >active</span
                >
              {/if}
            </div>
            <div class="font-mono text-[10px] opacity-50 mt-1.5 leading-relaxed">{st.description}</div>
          </button>
        {/each}
      </div>
      {#if previewStrategy}
        <div class="mt-4">
          <div class="font-mono text-[10px] uppercase tracking-widest opacity-40 mb-1.5">
            Self-prompt preview — {previewStrategy.tag} · {previewStrategy.label}
          </div>
          <pre
            class="font-mono text-[11px] leading-relaxed whitespace-pre-wrap rounded-lg border border-white/5 bg-konjo-black/60 p-3 opacity-80 max-h-72 overflow-auto">{previewStrategy.preview}</pre>
        </div>
      {/if}

      <!-- Adaptive escalation: climb S1→S4 as attempts keep failing -->
      <div class="mt-4 rounded-lg border border-white/5 bg-konjo-black/40 p-3">
        <div class="flex items-center justify-between gap-3">
          <div class="min-w-0">
            <div class="font-display text-[13px]">Adaptive escalation</div>
            <div class="font-mono text-[10px] opacity-50 mt-0.5 leading-relaxed">
              Climb one rung up the ladder each failed attempt — cheap retries first, heavier
              framing only when a task resists a fix.
            </div>
          </div>
          <button
            type="button"
            role="switch"
            aria-checked={snap.config.escalate_strategy}
            class="relative flex-shrink-0 w-11 h-6 rounded-full transition-colors"
            class:bg-konjo-jade={snap.config.escalate_strategy}
            class:bg-white={!snap.config.escalate_strategy}
            class:bg-opacity-10={!snap.config.escalate_strategy}
            on:click={() => snap && toggleEscalation(!snap.config.escalate_strategy)}
          >
            <span
              class="absolute top-0.5 left-0.5 w-5 h-5 rounded-full bg-white transition-transform"
              class:translate-x-5={snap.config.escalate_strategy}
            ></span>
          </button>
        </div>
        {#if snap.config.escalate_strategy}
          <div class="mt-3 flex items-center gap-1.5 flex-wrap">
            {#each snap.config.escalation_ladder as rung, i}
              <div class="flex items-center gap-1.5">
                {#if i > 0}<span class="opacity-30 font-mono text-[10px]">→</span>{/if}
                <div
                  class="rounded-md border border-white/10 bg-konjo-black/50 px-2 py-1 font-mono text-[10px]"
                >
                  <span class="opacity-40">#{rung.attempt}</span>
                  <span class={strategyColor(rung.tag)}>{rung.tag}</span>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </Panel>

    <!-- Schedules with the trust-level dropdown (the one writable control) -->
    <Panel title="Scheduled Loops" subtitle="set each loop's trust level">
      {#if snap.schedules.length === 0}
        <EmptyState title="No scheduled loops" detail="Add schedules from the Schedules tab." />
      {:else}
        <div class="space-y-2">
          {#each snap.schedules as s (s.id)}
            <div
              class="flex items-center gap-3 rounded-lg border border-white/5 bg-konjo-black/40 px-3 py-2.5"
            >
              <span
                class="w-2 h-2 rounded-full flex-shrink-0"
                class:bg-konjo-jade={s.enabled}
                class:bg-white={!s.enabled}
                class:opacity-30={!s.enabled}
              ></span>
              <div class="min-w-0 flex-1">
                <div class="font-mono text-[12px] truncate">{s.name}</div>
                <div class="font-mono text-[10px] opacity-40 truncate">{s.cron} · {s.goal}</div>
              </div>
              <div class="flex-shrink-0 w-44">
                <Dropdown
                  value={s.autonomy_level}
                  options={autonomyOptions}
                  on:change={(e) => changeAutonomy(s.id, e.detail)}
                />
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </Panel>

    <!-- Context: skills + rules -->
    <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
      <Panel title="Skills" subtitle="{snap.skills.length} discovered">
        {#if snap.skills.length === 0}
          <EmptyState title="No skills" detail=".claude/skills/*/SKILL.md" />
        {:else}
          <div class="space-y-2">
            {#each snap.skills as sk}
              <div class="rounded-md border border-white/5 bg-konjo-black/30 px-3 py-2">
                <div class="font-mono text-[12px] text-konjo-accent">{sk.name}</div>
                {#if sk.description}
                  <div class="font-mono text-[10px] opacity-50 mt-0.5 leading-relaxed">
                    {sk.description}
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </Panel>

      <Panel title="Rules" subtitle="{snap.rules.length} active">
        {#if snap.rules.length === 0}
          <EmptyState title="No rules" detail=".claude/rules/*.md" />
        {:else}
          <div class="flex flex-wrap gap-2">
            {#each snap.rules as r}
              <span
                class="font-mono text-[11px] rounded-md border border-white/10 bg-konjo-black/40 px-2.5 py-1 opacity-80"
                >{r.name}</span
              >
            {/each}
          </div>
        {/if}
      </Panel>
    </div>

    <!-- Guardrail gates -->
    <Panel title="Quality Gates" subtitle="Konjo three-wall framework — the loop's 'no'">
      <div class="space-y-2">
        {#each snap.gates as g}
          <div class="rounded-lg border border-white/5 bg-konjo-black/40 px-3 py-2.5">
            <div class="flex items-center gap-2">
              <span class="font-mono text-[11px] font-bold text-konjo-sun">{g.wall}</span>
              <span class="font-display text-[13px]">{g.name}</span>
            </div>
            <div class="font-mono text-[10px] opacity-50 mt-1 leading-relaxed">{g.checks}</div>
          </div>
        {/each}
      </div>
    </Panel>
  {/if}
</div>
