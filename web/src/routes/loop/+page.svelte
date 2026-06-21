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
    setScheduleAutonomy,
    type LoopSnapshot,
    type AutonomyOption
  } from '$lib/api';
  import Panel from '$lib/components/ui/Panel.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import Dropdown from '$lib/components/ui/Dropdown.svelte';
  import type { Option } from '$lib/stores/controls';

  let snap: LoopSnapshot | null = null;
  let loading = true;
  let loadError = '';
  let flash = '';

  async function refresh() {
    try {
      snap = await getLoopEngineering();
      loadError = '';
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'failed to load';
    } finally {
      loading = false;
    }
  }

  onMount(refresh);

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

  function fmtBudget(t: number): string {
    return t === 0 ? 'inherit global' : `${t.toLocaleString()} tokens`;
  }
</script>

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
