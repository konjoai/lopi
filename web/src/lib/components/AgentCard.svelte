<!--
  AgentCard — minimal status card for the sidebar.
  Click to make this the active agent (drives the Forge).
-->
<script lang="ts">
  import { PHASE_COLORS, type AgentState, selectAgent, activeAgentId } from '$lib/stores/agents';

  export let agent: AgentState;

  $: isActive = $activeAgentId === agent.id;
  $: phaseColor = PHASE_COLORS[agent.phase];

  function fmtElapsed(ms: number): string {
    const s = Math.floor(ms / 1000);
    if (s < 60) return `${s}s`;
    const m = Math.floor(s / 60);
    return `${m}m ${s % 60}s`;
  }

  function statusDot(s: string): string {
    if (s === 'running') return 'bg-konjo-jade animate-pulse';
    if (s === 'queued') return 'bg-konjo-sun';
    if (s === 'completed') return 'bg-konjo-jade/50';
    if (s === 'failed') return 'bg-konjo-rose';
    return 'bg-white/30';
  }
</script>

<button
  type="button"
  class="group block w-full text-left px-4 py-3 rounded-lg transition-all duration-300 border"
  class:border-white={false}
  style:border-color={isActive ? phaseColor : 'rgba(255,255,255,0.06)'}
  style:background={isActive ? 'rgba(255,255,255,0.03)' : 'transparent'}
  on:click={() => selectAgent(agent.id)}
>
  <div class="flex items-start gap-3">
    <span class="mt-1.5 w-1.5 h-1.5 rounded-full flex-shrink-0 {statusDot(agent.status)}"></span>

    <div class="flex-1 min-w-0">
      <div class="font-sans text-sm font-medium leading-tight truncate">
        {agent.goal}
      </div>
      <div class="mt-1 flex items-center gap-2 font-mono text-[10px] uppercase tracking-wider opacity-50">
        <span>{agent.repo}</span>
        <span>·</span>
        <span style:color={phaseColor} class="opacity-100">{agent.phase}</span>
        <span>·</span>
        <span>{fmtElapsed(agent.elapsedMs)}</span>
      </div>
    </div>
  </div>
</button>
