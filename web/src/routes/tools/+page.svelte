<script lang="ts">
  import { onMount } from 'svelte';
  import { listTools, registerTool, deleteTool, type ToolSpec } from '$lib/api';
  import Panel from '$lib/components/ui/Panel.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';

  let tools: ToolSpec[] = [];
  let loading = true;
  let loadError = '';
  let actionError = '';
  let flash = '';

  // Register form
  let formOpen = false;
  let name = '';
  let description = '';
  let parametersText = '{\n  "type": "object",\n  "properties": {}\n}';
  let timeoutMs = 30000;
  let retries = 0;
  let saving = false;

  async function refresh() {
    try {
      const r = await listTools();
      tools = r.tools;
      loadError = '';
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'failed to load';
    } finally {
      loading = false;
    }
  }

  async function save() {
    if (!name.trim() || !description.trim() || saving) return;
    let parameters: unknown;
    try {
      parameters = JSON.parse(parametersText);
    } catch {
      actionError = 'parameters must be valid JSON';
      return;
    }
    saving = true;
    actionError = '';
    try {
      await registerTool({
        name: name.trim(),
        description: description.trim(),
        parameters,
        timeout_ms: timeoutMs,
        retries
      });
      formOpen = false;
      name = '';
      description = '';
      flash = 'tool registered';
      setTimeout(() => (flash = ''), 2500);
      await refresh();
    } catch (e) {
      actionError = e instanceof Error ? e.message : 'register failed';
    } finally {
      saving = false;
    }
  }

  async function remove(t: ToolSpec) {
    actionError = '';
    try {
      await deleteTool(t.name);
      flash = `${t.name} deregistered`;
      setTimeout(() => (flash = ''), 2500);
      await refresh();
    } catch (e) {
      actionError = e instanceof Error ? e.message : 'delete failed';
    }
  }

  onMount(refresh);
</script>

<svelte:head><title>lopi · tools</title></svelte:head>

<div class="max-w-6xl mx-auto px-6 py-8 space-y-6">
  <Panel title="Tools" subtitle="durable tool registry · {tools.length} registered">
    <svelte:fragment slot="actions">
      {#if flash}
        <span class="font-mono text-[10px] uppercase tracking-widest text-konjo-jade animate-pulse">
          {flash}
        </span>
      {/if}
      <button
        type="button"
        on:click={() => (formOpen = !formOpen)}
        class="px-3 py-1 rounded font-mono text-[10px] uppercase tracking-widest bg-konjo-accent/10 text-konjo-accent border border-konjo-accent/40 hover:bg-konjo-accent/20 transition-colors"
      >
        {formOpen ? 'close' : '+ register'}
      </button>
    </svelte:fragment>

    {#if actionError}
      <p class="mb-3 font-mono text-xs" style:color="var(--konjo-rose)">{actionError}</p>
    {/if}

    {#if loading}
      <EmptyState title="loading…" />
    {:else if loadError}
      <EmptyState error title="backend unreachable" detail={loadError} />
    {:else if tools.length === 0 && !formOpen}
      <EmptyState
        title="no tools registered"
        detail="register a tool contract for agents to discover"
      />
    {:else}
      <div class="space-y-2">
        {#each tools as t (t.name)}
          <details class="rounded border border-white/10 px-3 py-2.5 group open:border-konjo-accent/30 transition-colors">
            <summary class="flex items-center gap-3 cursor-pointer list-none">
              <code class="font-mono text-sm text-konjo-accent">{t.name}</code>
              <span class="font-mono text-xs opacity-60 flex-1 truncate">{t.description}</span>
              <span class="font-mono text-[10px] opacity-40 flex-shrink-0 tabular-nums">
                {t.timeout_ms}ms · {t.retries} retr{t.retries === 1 ? 'y' : 'ies'}
              </span>
              <button
                type="button"
                on:click|preventDefault={() => remove(t)}
                class="px-2 py-1 rounded border border-white/10 text-konjo-rose hover:border-konjo-rose/50 hover:bg-konjo-rose/10 transition-colors font-mono text-[10px] uppercase tracking-widest flex-shrink-0"
              >
                remove
              </button>
            </summary>
            <pre
              class="mt-3 bg-black/40 rounded p-3 overflow-x-auto font-mono text-[11px] leading-relaxed">{JSON.stringify(
                t.parameters,
                null,
                2
              )}</pre>
          </details>
        {/each}
      </div>
    {/if}
  </Panel>

  {#if formOpen}
    <Panel title="Register tool" subtitle="contract only — enforcement is the caller's job">
      <form class="grid grid-cols-1 md:grid-cols-2 gap-4" on:submit|preventDefault={save}>
        <label class="flex flex-col gap-1">
          <span class="font-mono text-[9px] uppercase tracking-widest opacity-40">name</span>
          <input
            type="text"
            bind:value={name}
            placeholder="semantic-search"
            class="bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-sm font-mono py-1 placeholder:opacity-30 transition-colors"
          />
        </label>
        <label class="flex flex-col gap-1">
          <span class="font-mono text-[9px] uppercase tracking-widest opacity-40">description</span>
          <input
            type="text"
            bind:value={description}
            placeholder="search the vector index for related code"
            class="bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-sm font-mono py-1 placeholder:opacity-30 transition-colors"
          />
        </label>
        <label class="flex flex-col gap-1 md:col-span-2">
          <span class="font-mono text-[9px] uppercase tracking-widest opacity-40">
            parameters (JSON Schema)
          </span>
          <textarea
            bind:value={parametersText}
            rows={6}
            class="bg-black/40 border border-white/10 focus:border-konjo-accent rounded outline-none text-xs font-mono p-2 transition-colors"
          ></textarea>
        </label>
        <label class="flex flex-col gap-1">
          <span class="font-mono text-[9px] uppercase tracking-widest opacity-40">timeout (ms)</span>
          <input
            type="number"
            bind:value={timeoutMs}
            min={1}
            class="bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-sm font-mono py-1 transition-colors"
          />
        </label>
        <label class="flex flex-col gap-1">
          <span class="font-mono text-[9px] uppercase tracking-widest opacity-40">retries</span>
          <input
            type="number"
            bind:value={retries}
            min={0}
            max={255}
            class="bg-transparent border-b border-white/10 focus:border-konjo-accent outline-none text-sm font-mono py-1 transition-colors"
          />
        </label>
        <div class="md:col-span-2">
          <button
            type="submit"
            disabled={saving || !name.trim() || !description.trim()}
            class="px-4 py-1.5 rounded font-mono text-xs uppercase tracking-widest bg-konjo-accent/10 text-konjo-accent border border-konjo-accent/40 hover:bg-konjo-accent/20 disabled:opacity-30 transition-colors"
          >
            {saving ? 'registering…' : 'register'}
          </button>
        </div>
      </form>
    </Panel>
  {/if}
</div>
