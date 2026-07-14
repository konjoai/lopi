<!--
  ProvenanceChips — the card's origin chips (Creation-Flow-1 §4). Not a card
  component (the "one card component" rule is about not forking StackCard) —
  just the shared chip cluster the draft card and every committed card both
  render, so the two never drift.

  Color semantics match the templates dropdown's sections:
    • prompt template → a SUN chip with the template name, which *replaces* the
      teal alias chip (the template is that prompt's identity).
    • stack template  → a VIOLET chip with the template name, PLUS the card's
      own teal alias chip (each loop in a chain keeps its distinct preset).
    • no template     → today's teal alias chip, unchanged.

  Every chip has an explicit `svg { width; height }` — a missing size renders a
  full-size icon and blows the card apart (the mockup bug §4 calls out).
-->
<script lang="ts">
  import { ICONS } from './icons';

  export let alias: string | undefined = undefined;
  export let tpl: string | undefined = undefined;
  export let tplKind: 'prompt' | 'stack' | undefined = undefined;

  $: isPrompt = tplKind === 'prompt' && !!tpl;
  $: isStack = tplKind === 'stack' && !!tpl;
  // The teal alias chip shows for a stack-template loop and for a no-template
  // card, but never for a prompt template (its sun chip *is* the identity).
  $: showAlias = !!alias && !isPrompt;
</script>

{#if isPrompt}
  <span class="chip prompt" title="from prompt template">{@html ICONS.file}{tpl}</span>
{/if}
{#if isStack}
  <span class="chip stack" title="from stack template">{@html ICONS.layers}{tpl}</span>
{/if}
{#if showAlias}
  <span class="chip alias">{@html ICONS.wrench}:{alias}</span>
{/if}

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12.5px;
    border-radius: 7px;
    padding: 3px 10px;
  }
  .chip :global(svg) {
    width: 12px;
    height: 12px;
  }
  .chip.alias {
    color: var(--stack-teal, #00ffd4);
    border: 1px solid rgba(0, 255, 212, 0.4);
    background: rgba(0, 255, 212, 0.07);
  }
  .chip.prompt {
    color: var(--konjo-sun, #ffcc00);
    border: 1px solid rgba(255, 204, 0, 0.4);
    background: rgba(255, 204, 0, 0.08);
  }
  .chip.stack {
    color: var(--stack-violet, #b79bff);
    border: 1px solid rgba(183, 155, 255, 0.4);
    background: rgba(183, 155, 255, 0.08);
  }
</style>
