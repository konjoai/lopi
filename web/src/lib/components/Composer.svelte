<!--
  Composer — the chat input row pinned to the bottom of the agent pane: a growing
  prompt textarea (Enter sends, Shift+Enter inserts a newline) on the left and a
  send button on the right, with stop/retry surfacing when a session is mounted.
-->
<script lang="ts">
  export let hasAgent = false;
  export let isSubmitting = false;
  export let error = '';
  export let onSubmit: (text: string) => void = () => {};

  let value = '';

  function submit() {
    const text = value.trim();
    if (!text || isSubmitting) return;
    onSubmit(text);
    value = '';
  }

  function onKeydown(e: KeyboardEvent) {
    // Enter sends; Shift+Enter (or any modifier) inserts a newline.
    if (e.key === 'Enter' && !e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey) {
      e.preventDefault();
      submit();
    }
  }
</script>

<div class="composer">
  {#if error}
    <div class="err">{error}</div>
  {/if}
  <div class="row">
    <textarea
      bind:value
      on:keydown={onKeydown}
      rows="1"
      placeholder={hasAgent ? 'message this agent…  (Enter to send, Shift+Enter for newline)' : 'type a goal…'}
      disabled={isSubmitting}
    ></textarea>
    <button type="button" class="send" title="Send" on:click={submit} disabled={isSubmitting || !value.trim()}>
      {#if isSubmitting}⟳{:else}↗{/if}
    </button>
  </div>
</div>

<style>
  .composer {
    border-top: 1px solid rgba(255, 255, 255, 0.06);
    background: rgba(0, 0, 0, 0.18);
    padding: 0.5rem 0.6rem;
    flex-shrink: 0;
  }
  .err {
    color: var(--konjo-rose);
    font-family: var(--font-mono, monospace);
    font-size: 0.65rem;
    margin-bottom: 0.35rem;
  }
  .row {
    display: flex;
    align-items: flex-end;
    gap: 0.4rem;
  }
  textarea {
    flex: 1;
    min-width: 0;
    resize: none;
    max-height: 7rem;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 10px;
    padding: 0.45rem 0.6rem;
    font-family: var(--font-mono, monospace);
    font-size: 0.78rem;
    line-height: 1.4;
    color: var(--konjo-paper);
    outline: none;
    transition: border-color var(--dur-fast) var(--ease-out-expo);
    field-sizing: content;
  }
  textarea:focus {
    border-color: var(--konjo-ice);
  }
  textarea::placeholder {
    opacity: 0.32;
  }
  .send {
    flex-shrink: 0;
    width: 2.1rem;
    height: 2.1rem;
    border-radius: 10px;
    display: grid;
    place-items: center;
    font-size: 1rem;
    color: var(--konjo-black);
    background: var(--konjo-ice);
    border: 1px solid transparent;
    transition: filter var(--dur-fast), opacity var(--dur-fast);
  }
  .send:disabled {
    opacity: 0.35;
  }
  .send:not(:disabled):hover {
    filter: brightness(1.1);
  }
</style>
