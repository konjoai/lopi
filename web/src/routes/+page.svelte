<!--
  Hero view — the Forge centerpiece flanked by surgical data panels.
  Mobile: stacks vertically with a smaller Forge.
-->
<script lang="ts">
  import Forge from '$lib/forge/Forge.svelte';
  import PhaseWheel from '$lib/components/PhaseWheel.svelte';
  import TokenGauge from '$lib/components/TokenGauge.svelte';
  import AgentCard from '$lib/components/AgentCard.svelte';
  import ThoughtStream from '$lib/components/ThoughtStream.svelte';
  import LogStream from '$lib/components/LogStream.svelte';
  import CostCounter from '$lib/components/CostCounter.svelte';
  import { agents, activeAgent, stats, PHASE_COLORS } from '$lib/stores/agents';

  // Reactive — drives the Forge from the active agent
  $: phaseColor = $activeAgent ? PHASE_COLORS[$activeAgent.phase] : '#00d4ff';
  $: pressure = $activeAgent?.pressure ?? 0.3;
  $: activity = $activeAgent?.activity ?? 0.4;
  $: health = $activeAgent?.health ?? 0.85;
  $: agentList = Array.from($agents.values());
</script>

<div class="grid grid-cols-12 gap-6 px-6 py-8 max-w-[1600px] mx-auto">
  <!-- ── Left rail: agent list ─────────────────────────────────────────── -->
  <aside class="col-span-12 lg:col-span-3 order-2 lg:order-1 space-y-3">
    <div class="flex items-baseline justify-between mb-2">
      <h2 class="font-mono text-[10px] uppercase tracking-widest opacity-50">agents</h2>
      <span class="font-mono text-[10px] tabular-nums opacity-50">
        {$stats.running} running · {$stats.total} total
      </span>
    </div>

    {#each agentList as agent (agent.id)}
      <AgentCard {agent} />
    {/each}

    {#if agentList.length === 0}
      <div class="text-sm opacity-40 italic font-mono">no active agents</div>
    {/if}
  </aside>

  <!-- ── Center: the Forge ─────────────────────────────────────────────── -->
  <section class="col-span-12 lg:col-span-6 order-1 lg:order-2 flex flex-col items-center gap-8">
    <!-- Phase wheel above the Forge -->
    <div class="-mb-6 relative z-20">
      <PhaseWheel phase={$activeAgent?.phase ?? 'Boot'} size={140} />
    </div>

    <!-- The Forge -->
    <div class="relative">
      <Forge {pressure} {phaseColor} {activity} {health} size={420} />

      <!-- Phase label drifting beneath the sphere -->
      <div class="absolute inset-x-0 -bottom-12 text-center">
        <div class="font-display text-3xl tracking-tight" style:color={phaseColor}>
          {$activeAgent?.phase ?? '—'}
        </div>
      </div>
    </div>

    <!-- Goal + thought stream -->
    <div class="mt-12 max-w-2xl text-center space-y-3">
      <div class="font-display text-lg leading-snug">
        {$activeAgent?.goal ?? 'no agent selected'}
      </div>
      <div class="font-mono text-[10px] uppercase tracking-widest opacity-50">
        {$activeAgent?.repo ?? ''}
        {#if $activeAgent?.branch}
          · {$activeAgent.branch}
        {/if}
        {#if $activeAgent}
          · attempt {$activeAgent.attempt}
        {/if}
      </div>

      {#if $activeAgent?.thought}
        <div class="mt-6 flex justify-center">
          <ThoughtStream thought={$activeAgent.thought} />
        </div>
      {/if}
    </div>
  </section>

  <!-- ── Right rail: gauges + cost ─────────────────────────────────────── -->
  <aside class="col-span-12 lg:col-span-3 order-3 flex flex-col items-end gap-8 pt-4">
    <CostCounter cost={$activeAgent?.cost ?? 0} cap={1.0} />

    <div class="flex items-end gap-6">
      <div class="flex flex-col items-center gap-2">
        <span class="font-mono text-[10px] uppercase tracking-widest opacity-50">activity</span>
        <div
          class="w-20 h-20 rounded-full ring-1 ring-white/10 flex items-center justify-center transition-all duration-500"
          style:background={`radial-gradient(circle, ${phaseColor}33 0%, transparent 70%)`}
          style:box-shadow={`0 0 ${20 * activity}px ${phaseColor}66`}
        >
          <span class="font-mono text-sm tabular-nums">
            {Math.round(activity * 100)}
          </span>
        </div>
      </div>

      <TokenGauge {pressure} height={200} />
    </div>

    <!-- Aggregate stats -->
    <div class="w-full grid grid-cols-2 gap-4 mt-4">
      {#each [['running', $stats.running, 'var(--konjo-jade)'], ['queued', $stats.queued, 'var(--konjo-sun)'], ['done', $stats.completed, 'var(--konjo-paper)'], ['failed', $stats.failed, 'var(--konjo-rose)']] as [label, val, c]}
        <div class="text-right">
          <div class="font-mono text-[10px] uppercase tracking-widest opacity-50">{label}</div>
          <div class="font-display text-2xl tabular-nums" style:color={c}>{val}</div>
        </div>
      {/each}
    </div>
  </aside>

  <!-- ── Bottom: log stream ─────────────────────────────────────────────── -->
  <div class="col-span-12 order-4 mt-12">
    <div class="font-mono text-[10px] uppercase tracking-widest opacity-50 mb-2">log stream</div>
    <LogStream />
  </div>
</div>
