<!--
  ProposalCard — the "ghost card" successor-proposal (Ghost Card in the
  Stack handoff, direction 1a). Renders a pane's `StackProposal` directly
  beneath the card that spawned it: dashed violet while `'proposed'`, a
  one-line dismissible strip once `'discarded'`.

  Deliberately not a StackCard.svelte branch — a proposal isn't a `CardStatus`
  (see `StackProposal`'s doc comment in `stores/stack.ts`), so it gets its own
  small component rather than growing StackCard's already-large branch count.
  The goal textarea reuses StackCard's exact `mdinput` pattern (`autoGrow`,
  same classes) — there is no separate edit mode; the user clicks straight
  into the text, same as a committed card's own goal field.
-->
<script lang="ts">
  import {
    type StackProposal,
    updateProposalGoalInPane,
    discardProposalInPane,
    undoDiscardProposalInPane,
    acceptProposalInPane
  } from '$lib/stores/stack';
  import { ICONS } from './icons';
  import { autoGrow } from './autoGrow';
  import ProvenanceChips from './ProvenanceChips.svelte';

  export let proposal: StackProposal;
  export let paneKey: string;

  $: discarded = proposal.status === 'discarded';

  function onGoalInput(e: Event): void {
    updateProposalGoalInPane(paneKey, (e.currentTarget as HTMLTextAreaElement).value);
  }
  function accept(): void {
    acceptProposalInPane(paneKey);
  }
  function discard(): void {
    discardProposalInPane(paneKey);
  }
  function undo(): void {
    undoDiscardProposalInPane(paneKey);
  }
</script>

{#if discarded}
  <div class="propstrip" role="listitem">
    <span class="txt">proposal discarded</span>
    <button type="button" class="undo" on:click={undo}>undo</button>
  </div>
{:else}
  <div class="pc" role="listitem">
    <span class="runtag">proposed &middot; loop {proposal.loopNumber}/{proposal.loopTotal}</span>
    <div class="spec">
      <ProvenanceChips
        alias={proposal.alias}
        tpl={proposal.tpl}
        tplKind={proposal.tplKind}
        repoLabel={proposal.repoLabel}
      />
      <textarea
        class="md mdinput"
        value={proposal.goal}
        on:input={onGoalInput}
        use:autoGrow
        rows="1"
        spellcheck="false"
        aria-label="edit proposed prompt"
      ></textarea>
    </div>
    <div class="cardbar">
      <span class="sp"></span>
      <button type="button" class="ib accept" on:click={accept} title="accept — add to stack">
        {@html ICONS.check}<span class="lbl">accept</span>
      </button>
      <button type="button" class="ib discard" on:click={discard} title="discard">
        {@html ICONS.x}<span class="lbl">discard</span>
      </button>
    </div>
  </div>
{/if}

<style>
  .pc {
    position: relative;
    background: var(--konjo-card, var(--k-ext-surface-panel));
    border: 1.5px dashed rgb(var(--k-ext-violet-testing-rgb) / 0.55);
    border-radius: 9px;
    padding: 13px 14px;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    box-shadow: inset 0 1px 0 rgb(var(--k-wash-rgb) / 0.08);
  }
  .runtag {
    position: absolute;
    top: -10px;
    right: 14px;
    font-size: 9px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    background: var(--konjo-black, var(--k-ext-black-fallback));
    border: 1px solid var(--k-border-subtle);
    border-radius: 3px;
    padding: 2px 8px;
    display: inline-flex;
    align-items: center;
    color: var(--k-text-muted);
    z-index: 2;
  }
  .spec {
    font-size: 14px;
    line-height: 1.5;
    margin-top: 3px;
    display: flex;
    align-items: center;
    gap: 9px;
    flex-wrap: wrap;
  }
  /* Byte-for-byte the same `mdinput` rest/hover/focus treatment as
     StackCard.svelte's committed-card goal field — Svelte scopes styles per
     component, so this can't be shared directly, only kept in sync by eye. */
  .spec .mdinput {
    flex: 1 1 100%;
    width: 100%;
    min-width: 120px;
    display: block;
    resize: none;
    overflow: hidden;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 5px;
    margin: -3px -6px;
    padding: 2px 6px;
    color: rgb(var(--k-text-primary-rgb) / 0.46);
    font-family: inherit;
    font-size: inherit;
    line-height: inherit;
    outline: none;
    transition:
      border-color 0.12s,
      background 0.12s,
      color 0.12s;
  }
  .spec .mdinput:hover {
    border-color: rgb(var(--k-wash-rgb) / 0.11);
    background: rgb(var(--k-wash-rgb) / 0.02);
  }
  .spec .mdinput:focus {
    border-color: rgb(var(--k-chip-alias-rgb) / 0.4);
    background: rgb(var(--k-chip-alias-rgb) / 0.03);
    color: var(--konjo-paper, var(--k-text-primary));
  }
  .cardbar {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 12px;
  }
  .sp {
    flex: 1;
  }
  .ib {
    height: 29px;
    padding: 0 12px;
    border-radius: 6px;
    border: 1px solid var(--k-border-interactive);
    background: transparent;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    font-weight: 700;
    transition: 0.12s;
  }
  .ib :global(svg) {
    width: 14px;
    height: 14px;
  }
  .ib.accept {
    color: var(--konjo-jade, var(--k-preset-benchmark));
    border-color: rgb(var(--k-preset-benchmark-rgb) / 0.4);
    background: rgb(var(--k-preset-benchmark-rgb) / 0.1);
  }
  .ib.accept:hover {
    border-color: rgb(var(--k-preset-benchmark-rgb) / 0.7);
    background: rgb(var(--k-preset-benchmark-rgb) / 0.16);
  }
  .ib.discard {
    color: rgb(var(--k-ext-rose-soft-rgb) / 0.8);
    border-color: rgb(var(--k-danger-rgb) / 0.3);
    background: transparent;
  }
  .ib.discard:hover {
    color: var(--konjo-rose, var(--k-danger));
    border-color: rgb(var(--k-danger-rgb) / 0.6);
    background: rgb(var(--k-danger-rgb) / 0.08);
  }
  /* Discarded — collapses to a one-line strip, dashed outline echoing the
     proposal's own border style so it still reads as "part of this gap"
     rather than a generic deleted-row placeholder. */
  .propstrip {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 34px;
    border: 1px dashed rgb(var(--k-wash-rgb) / 0.14);
    border-radius: 9px;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    font-size: 11px;
  }
  .propstrip .txt {
    color: rgb(var(--k-text-primary-rgb) / 0.4);
  }
  .propstrip .undo {
    background: none;
    border: none;
    padding: 0;
    color: var(--konjo-ice, var(--k-chip-repo));
    cursor: pointer;
    font-family: inherit;
    font-size: inherit;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .propstrip .undo:hover {
    color: var(--konjo-paper, var(--k-text-primary));
  }
</style>
