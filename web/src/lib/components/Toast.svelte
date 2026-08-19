<!--
  Toast — a generic undo-toast host (round 2, item 1). Mount once per page
  that needs it (currently just `/stacks`); renders every live `toastStore`
  entry stacked bottom-center, each auto-dismissing on its own timer.
-->
<script lang="ts">
  import { toasts, dismissToast } from '$lib/stores/toastStore';

  function act(id: string, action: () => void) {
    action();
    dismissToast(id);
  }
</script>

<div class="toaststack" aria-live="polite">
  {#each $toasts as t (t.id)}
    <div class="toast">
      <span class="msg">{t.message}</span>
      {#if t.action}
        {@const action = t.action}
        <button type="button" class="undo" on:click={() => act(t.id, action.onClick)}>{action.label}</button>
      {/if}
    </div>
  {/each}
</div>

<style>
  .toaststack {
    position: fixed;
    left: 50%;
    bottom: 28px;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 200;
    pointer-events: none;
  }
  .toast {
    pointer-events: auto;
    display: inline-flex;
    align-items: center;
    gap: 14px;
    padding: 11px 16px;
    border-radius: 9px;
    background: var(--konjo-panel, var(--k-surface-raised));
    border: 1px solid rgb(var(--k-wash-rgb) / 0.14);
    box-shadow: 0 12px 32px rgb(var(--k-shadow-rgb) / 0.5);
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    font-size: 12px;
    color: var(--konjo-paper, var(--k-text-primary));
    animation: toastin 0.16s ease-out;
  }
  .toast .msg {
    white-space: nowrap;
  }
  .toast .undo {
    background: none;
    border: none;
    padding: 0;
    color: var(--konjo-ice, var(--k-chip-repo));
    font-family: inherit;
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
    white-space: nowrap;
  }
  .toast .undo:hover {
    color: var(--k-ext-plasma);
  }
  @keyframes toastin {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .toast {
      animation: none;
    }
  }
</style>
