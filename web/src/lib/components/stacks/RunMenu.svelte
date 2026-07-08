<!--
  RunMenu — the run-stack chevron's dropdown. Every item is a stub: no
  pause/drain/bump signals exist server-side yet (see NEXT.md), so this
  wires open/close and nothing else. Closes on outside-click or Escape.
-->
<script lang="ts">
  import { ICONS } from './icons';

  export let onClose: () => void;

  const items = [
    { icon: ICONS.play, name: 'Run now', sub: 'start now' },
    { icon: ICONS.check, name: 'Run once', sub: 'one pass each' },
    { icon: ICONS.cron, name: 'Schedule stack', sub: 'one cron' },
    { icon: ICONS.flask, name: 'Dry run', sub: 'validate only' }
  ];

  // TODO(backend): run-stack execution needs pause/drain/bump signals that
  // don't exist yet — every item is a no-op stub this slice.
  function pick() {
    onClose();
  }

  function onOutside(e: MouseEvent) {
    const el = e.target as HTMLElement;
    if (el.closest('.runmenu') || el.closest('.runchev')) return;
    onClose();
  }
  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') onClose();
  }
</script>

<svelte:window on:keydown={onKeydown} />
<svelte:body on:mousedown|capture={onOutside} />

<div class="runmenu">
  {#each items as it (it.name)}
    <button type="button" class="rm" on:click={pick}>
      {@html it.icon}<span class="rmn">{it.name}</span><span class="rms">{it.sub}</span>
    </button>
  {/each}
</div>

<style>
  .runmenu {
    position: absolute;
    bottom: 72px;
    left: 50%;
    transform: translateX(-50%);
    width: 320px;
    max-width: calc(100vw - 24px);
    background: var(--konjo-panel, #0a0d0f);
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 11px;
    box-shadow: 0 20px 55px rgba(0, 0, 0, 0.8);
    overflow: hidden;
    z-index: 40;
  }
  .rm {
    display: flex;
    align-items: center;
    gap: 13px;
    padding: 13px 17px;
    cursor: pointer;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
    width: 100%;
    background: transparent;
    border-left: none;
    border-right: none;
    border-top: none;
    text-align: left;
  }
  .rm:last-child {
    border-bottom: none;
  }
  .rm:hover {
    background: rgba(255, 255, 255, 0.03);
  }
  .rm :global(svg) {
    width: 16px;
    height: 16px;
    color: var(--konjo-flame);
    flex: 0 0 auto;
  }
  .rm .rmn {
    font-family: var(--font-sans, 'Space Grotesk', sans-serif);
    font-size: 14px;
    color: var(--konjo-paper, #f5f5f5);
    flex: 1;
  }
  .rm .rms {
    font-family: var(--font-mono, monospace);
    font-size: 10px;
    color: rgba(245, 245, 245, 0.28);
  }
</style>
