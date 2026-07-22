<script lang="ts">
  import { onMount } from 'svelte';
  import { getConfig, getVersion } from '$lib/api';
  import Panel from '$lib/components/ui/Panel.svelte';
  import EmptyState from '$lib/components/ui/EmptyState.svelte';
  import { theme, setTheme, THEMES } from '$lib/stores/theme';

  let config: Record<string, unknown> | null = null;
  let source = '';
  let version: { service: string; version: string; uptime_secs: number } | null = null;
  let loadError = '';
  let loading = true;
  let view: 'tree' | 'raw' = 'tree';

  async function refresh() {
    try {
      const [c, v] = await Promise.all([getConfig(), getVersion()]);
      config = c.config;
      source = c.source;
      version = v;
      loadError = '';
    } catch (e) {
      loadError = e instanceof Error ? e.message : 'failed to load';
    } finally {
      loading = false;
    }
  }

  function fmtUptime(secs: number): string {
    const h = Math.floor(secs / 3600);
    const m = Math.floor((secs % 3600) / 60);
    if (h > 0) return `${h}h ${m}m`;
    return `${m}m ${secs % 60}s`;
  }

  // Flatten the config object into [path, value] leaf rows for the tree view.
  function flatten(obj: unknown, prefix = ''): [string, string][] {
    if (obj === null || obj === undefined) return [[prefix, 'null']];
    if (typeof obj !== 'object') return [[prefix, String(obj)]];
    if (Array.isArray(obj)) {
      if (obj.length === 0) return [[prefix, '[]']];
      return obj.flatMap((v, i) => flatten(v, `${prefix}[${i}]`));
    }
    const entries = Object.entries(obj as Record<string, unknown>);
    if (entries.length === 0) return [[prefix, '{}']];
    return entries.flatMap(([k, v]) => flatten(v, prefix ? `${prefix}.${k}` : k));
  }

  $: rows = config ? flatten(config) : [];

  onMount(refresh);
</script>

<svelte:head><title>lopi · config</title></svelte:head>

<div class="max-w-6xl mx-auto px-6 py-8 space-y-6">
  <!-- Header -->
  <div class="flex items-end justify-between flex-wrap gap-4">
    <div>
      <h1 class="font-display text-2xl">Configuration</h1>
      <p class="font-mono text-[11px] uppercase tracking-widest opacity-50 mt-1">
        app settings · theme · effective config
      </p>
    </div>
  </div>

  <!-- Server identity -->
  <Panel title="Server" subtitle="identity + uptime">
    {#if version}
      <div class="flex flex-wrap gap-8 font-mono text-sm">
        <div>
          <span class="opacity-40 text-[10px] uppercase tracking-widest block">service</span>
          {version.service}
        </div>
        <div>
          <span class="opacity-40 text-[10px] uppercase tracking-widest block">version</span>
          v{version.version}
        </div>
        <div>
          <span class="opacity-40 text-[10px] uppercase tracking-widest block">uptime</span>
          {fmtUptime(version.uptime_secs)}
        </div>
      </div>
    {:else if loading}
      <EmptyState title="loading…" />
    {:else}
      <EmptyState error title="backend unreachable" detail={loadError} />
    {/if}
  </Panel>

  <!-- Appearance — browser-local Konjo theme accent -->
  <Panel title="Appearance" subtitle="accent theme · stored in this browser only">
    <div class="flex gap-3">
      {#each THEMES as t (t.id)}
        <button
          type="button"
          on:click={() => setTheme(t.id)}
          class="flex items-center gap-2.5 px-4 py-2.5 rounded-lg border transition-all duration-200"
          class:scale-105={$theme === t.id}
          style:border-color={$theme === t.id ? t.swatch : 'rgba(255,255,255,0.1)'}
          style:box-shadow={$theme === t.id ? `0 0 16px ${t.swatch}33` : 'none'}
        >
          <span
            class="w-3.5 h-3.5 rounded-full"
            style:background={t.swatch}
            style:box-shadow={`0 0 8px ${t.swatch}`}
          ></span>
          <span class="font-mono text-xs uppercase tracking-widest">{t.label}</span>
        </button>
      {/each}
    </div>
  </Panel>

  <!-- Effective config -->
  <Panel
    title="Configuration"
    subtitle={source === 'file' ? 'lopi.toml · secrets redacted · read-only' : 'no config file found'}
  >
    <svelte:fragment slot="actions">
      <div class="flex gap-1 font-mono text-[10px] uppercase tracking-widest">
        <button
          type="button"
          on:click={() => (view = 'tree')}
          class="px-2 py-1 rounded border transition-colors"
          class:text-konjo-accent={view === 'tree'}
          class:opacity-40={view !== 'tree'}
          style:border-color={view === 'tree' ? 'var(--konjo-accent)' : 'rgba(255,255,255,0.1)'}
        >
          tree
        </button>
        <button
          type="button"
          on:click={() => (view = 'raw')}
          class="px-2 py-1 rounded border transition-colors"
          class:text-konjo-accent={view === 'raw'}
          class:opacity-40={view !== 'raw'}
          style:border-color={view === 'raw' ? 'var(--konjo-accent)' : 'rgba(255,255,255,0.1)'}
        >
          raw
        </button>
      </div>
    </svelte:fragment>

    {#if loading}
      <EmptyState title="loading…" />
    {:else if loadError}
      <EmptyState error title="backend unreachable" detail={loadError} />
    {:else if !config}
      <EmptyState
        title="no config file"
        detail="lopi is running on defaults — create lopi.toml to customize"
      />
    {:else if view === 'raw'}
      <pre
        class="bg-black/40 rounded p-4 overflow-x-auto font-mono text-[11px] leading-relaxed max-h-[60vh] overflow-y-auto">{JSON.stringify(
          config,
          null,
          2
        )}</pre>
    {:else}
      <div class="overflow-x-auto -mx-4 -mb-4">
        <table class="w-full text-left font-mono text-xs">
          <tbody>
            {#each rows as [path, value] (path)}
              <tr class="border-b border-white/5 hover:bg-white/5 transition-colors">
                <td class="px-4 py-2 opacity-60 whitespace-nowrap">{path}</td>
                <td class="px-4 py-2 break-all">
                  <span
                    style:color={value === '***'
                      ? 'var(--konjo-rose)'
                      : value === 'true' || value === 'false'
                        ? 'var(--konjo-sun)'
                        : /^-?\d+(\.\d+)?$/.test(value)
                          ? 'var(--konjo-jade)'
                          : 'var(--konjo-paper)'}
                  >
                    {value}
                  </span>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  </Panel>
</div>
