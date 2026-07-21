<!--
  StackPane — one pane's chrome: header (logo + title + status dot + inert
  X), top composer (new prompts prepend), the card stack itself (flowing
  down to the currently-executing loop at the bottom), and the purple stack
  control area (`StackControlDock.svelte` — loop/schedule/guardrails/evals/
  config for the whole chain, plus the real run-stack action; see Stack-1).
  Two (or more, since Stack-1 added `duplicateStack`) of these render
  side-by-side in `/stacks` and are fully independent — no cross-pane card
  drag this slice (whole-*stack* reordering is in scope, via the dock).
-->
<script lang="ts">
  import { type StackPaneState, perLoopScheduleGoverned, paneIsBare, executionOrder } from '$lib/stores/stack';
  import type { Option } from '$lib/stores/controls';
  import StackCard from './StackCard.svelte';
  import StackConnector from './StackConnector.svelte';
  import ProposalConnector from './ProposalConnector.svelte';
  import ProposalCard from './ProposalCard.svelte';
  import StackOutput from './StackOutput.svelte';
  import StackControlDock from './StackControlDock.svelte';
  import { ICONS } from './icons';
  import { orbStateForCard } from '$lib/forge/cardOrb';
  import { agents, permissionWaiting } from '$lib/stores/agents';
  import { draggingPane, armedPaneKey } from './dnd';

  export let pane: StackPaneState;
  export let index: number;
  export let repoOptions: Option[] = [];
  /** Close this pane. Null keeps the header X inert (e.g. a lone pane). */
  export let onClose: (() => void) | null = null;

  $: paneDefaults = pane.config.defaults;
  $: scheduleGoverned = perLoopScheduleGoverned(pane.config);
  // An empty pane is a *bare* box (composer + idle orb) that reads like the
  // old Forge pane; the purple stack control dock and inter-card connectors
  // appear as soon as the pane holds its first card.
  $: bare = paneIsBare(pane);
  // Ghost Card in the Stack: "proposed after loop N" names the spawning
  // card by its position in the actual run order, not its array index (the
  // array is newest-first — the reverse of execution order, see
  // `executionOrder`'s doc comment).
  $: runOrder = executionOrder(pane.cards);

  // ── whole-stack drag (Stack-1): armed by StackControlDock.svelte's grip
  //    handle (mousedown/mouseup on `armedPaneKey`, module-scope since the
  //    handle and this root are different components — see dnd.ts), same
  //    shape as StackCard.svelte's own armDrag/disarmDrag. `.pane` itself is
  //    the actual drag source only while armed, so the browser's native drag
  //    tracks a target big enough to reliably keep the cursor over —
  //    previously the handle button itself was the (tiny, ~14px) drag
  //    source, which is what made real drags lose the cursor and snap back
  //    on drop. The drop-target logic (before/after by cursor Y) stays in
  //    StackControlDock.svelte, unchanged.
  $: paneDraggable = $armedPaneKey === pane.key;

  function onPaneDragStart(e: DragEvent) {
    draggingPane.set({ paneKey: pane.key, index });
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
  }
  function onPaneDragEnd() {
    draggingPane.set(null);
    armedPaneKey.set(null);
  }
</script>

<div
  class="pane"
  class:dragging={$draggingPane?.paneKey === pane.key}
  role="listitem"
  draggable={paneDraggable}
  on:dragstart={onPaneDragStart}
  on:dragend={onPaneDragEnd}
>
  <div class="panehead">
    <span class="logo">{@html ICONS.mark}</span>
    <span class="ptitle">{pane.title}</span>
    <span class="hsp"></span>
    <span class="hdot"></span>
    <button
      class="hx"
      class:live={onClose}
      type="button"
      title={onClose ? 'close pane' : 'close'}
      disabled={!onClose}
      on:click={() => onClose?.()}
    >{@html ICONS.x}</button>
  </div>

  <div class="panestack">
    <!-- Creation-Flow-1: the draft card *is* the composer. Pinned at the top;
         the committed cards flow down below it toward the currently-executing
         loop at the bottom. The draft lives on `pane.draft` (never in
         `pane.cards`), so it's excluded from run/reorder/loop-count. -->
    <div class="loopwrap draftwrap">
      <StackCard
        card={pane.draft}
        paneKey={pane.key}
        index={-1}
        {paneDefaults}
        {repoOptions}
        {scheduleGoverned}
      />
    </div>

    {#if pane.cards.length > 0}
      <div class="draftconn" aria-hidden="true"><span class="dcline"></span></div>
      {#each pane.cards as card, i (card.id)}
        <!-- Computed once here (not inside StackCard/StackOutput) and set as
             `--orb` on the shared `.loopwrap` ancestor, so the card's border
             and — when attached — the live-output panel's border both read
             the same custom property. -->
        {@const orb = orbStateForCard(card.taskId, $agents, $permissionWaiting)}
        {#if card.taskId}
          <!-- Once a card has ever run, its output stays reachable regardless
               of current status (previously gated on `status === 'running'`,
               so the log vanished the instant a run finished — `StackOutput`
               itself relabels to "logs" once it's no longer live).

               The border+radius+animation live HERE, on the wrapper, not on
               `.pc`/`.output` individually (both are borderless below) — two
               separately-animated elements can share the exact same color
               and keyframes and *still* visibly desync, because each one's
               `animation` starts counting from whenever IT individually
               mounted/gained the class, not from a shared clock. A single
               animated border on their shared ancestor can't be out of sync
               with itself, and it also removes the seam a `.pc` bottom
               border + `.output` (no top border) used to draw between them. -->
          <div
            class="loopwrap hasout"
            class:running={card.status === 'running'}
            class:queued={card.status === 'queued'}
            class:done={card.status === 'done'}
            class:blocked={card.status === 'blocked'}
            style="--orb:{orb.glowColor}"
          >
            <StackCard {card} paneKey={pane.key} index={i} {paneDefaults} {repoOptions} {scheduleGoverned} />
            <StackOutput taskId={card.taskId} isRunning={card.status === 'running'} />
          </div>
        {:else}
          <div class="loopwrap" style="--orb:{orb.glowColor}">
            <StackCard {card} paneKey={pane.key} index={i} {paneDefaults} {repoOptions} {scheduleGoverned} />
          </div>
        {/if}
        {#if pane.proposal && pane.proposal.afterCardId === card.id}
          <ProposalConnector loopNumber={runOrder.findIndex((c) => c.id === card.id) + 1} />
          <ProposalCard proposal={pane.proposal} paneKey={pane.key} />
        {/if}
        {#if i < pane.cards.length - 1}
          <StackConnector {card} paneKey={pane.key} index={i} {scheduleGoverned} />
        {/if}
      {/each}
    {/if}
  </div>

  <!-- Previously gated behind `{#if !bare}` — an empty pane showed only the
       composer, with no way to set stack defaults/schedule/guardrails or add
       a whole stack template until the first prompt existed. The dock is now
       always present so those controls (and stack templates) can be set up
       before writing any prompt, not just after. -->
  <StackControlDock {pane} {index} {repoOptions} />
</div>

<style>
  .pane {
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 14px;
    background: var(--konjo-panel, #0a0d0f);
    position: relative;
    transition: opacity 0.12s;
    /* Fills its auto-tiling TileGrid cell; the card stack scrolls internally so
       a tall stack never blows out the grid. */
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .pane.dragging {
    opacity: 0.4;
  }
  .panehead {
    flex: 0 0 auto;
  }
  .panehead {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 14px 18px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.05);
  }
  .panehead .logo {
    color: var(--konjo-flame);
    display: inline-flex;
  }
  .panehead .logo :global(svg) {
    width: 19px;
    height: 19px;
  }
  .panehead .ptitle {
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    letter-spacing: 0.16em;
    text-transform: uppercase;
    color: var(--konjo-paper, #f5f5f5);
  }
  .panehead .hsp {
    flex: 1;
  }
  .panehead .hdot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: rgba(245, 245, 245, 0.28);
  }
  .panehead .hx {
    background: none;
    border: none;
    color: rgba(245, 245, 245, 0.28);
    cursor: not-allowed;
    display: inline-flex;
  }
  .panehead .hx.live {
    cursor: pointer;
  }
  .panehead .hx.live:hover {
    color: var(--konjo-rose, #ff0066);
  }
  .panehead .hx :global(svg) {
    width: 16px;
    height: 16px;
  }
  .panestack {
    padding: 24px 18px 8px;
    flex: 1 1 auto;
    overflow-y: auto;
    min-height: 0;
  }
  /* The short connector between the pinned draft and the committed stack —
     purely visual (unlike StackConnector, no "add between" affordance here). */
  .draftconn {
    position: relative;
    height: 30px;
    margin: 2px 0;
  }
  .draftconn .dcline {
    position: absolute;
    left: 50%;
    top: 0;
    bottom: 0;
    border-left: 2px dashed rgba(245, 245, 245, 0.28);
    transform: translateX(-1px);
  }
  /* `.loopwrap.hasout` owns the ENTIRE border for a card with output
     attached — `.pc` and `.output` are borderless inside it (see below), so
     there's exactly one outline and (when running) exactly one animation
     instance, not two independently-clocked ones. Radius resets keep each
     child's own background from showing a stray rounded corner at the seam
     the wrapper's single border now spans. */
  .loopwrap.hasout {
    border-radius: 9px;
    /* No `overflow: hidden` here — `.pc`'s bottom corners are already
       forced square just below, and `.output`'s own corners already match
       this radius, so nothing actually needs clipping for the outline to
       read as one seamless shape. Adding it clipped the "DONE"/"RUNNING"
       runtag badge, which is deliberately positioned to poke above the
       card's own top edge (`.runtag { top: -10px }` in StackCard.svelte). */
  }
  .loopwrap.hasout :global(.pc) {
    border: none !important;
    border-bottom-left-radius: 0;
    border-bottom-right-radius: 0;
  }
  .loopwrap.hasout :global(.output) {
    border: none !important;
  }
  .loopwrap.hasout.queued {
    border: 1px solid color-mix(in srgb, var(--orb) 40%, transparent);
  }
  .loopwrap.hasout.done {
    border: 1px solid color-mix(in srgb, var(--orb) 35%, transparent);
  }
  /* Blocked/error (round 2, item 3) — static rose, mirrors
     `StackCard.svelte`'s identical `.pc.blocked` fixed-color rationale
     (durable `card.status`, not a live `--orb` lookup). */
  .loopwrap.hasout.blocked {
    border: 1px solid rgba(255, 0, 102, 0.45);
  }
  .loopwrap.hasout.running {
    border: 1px solid color-mix(in srgb, var(--orb) 45%, transparent);
    animation: edgeflash 5s ease-in-out infinite;
  }
  /* Kept byte-for-byte identical to StackCard.svelte's own `edgeflash` (its
     copy still drives borderless-output cards, i.e. `.pc.running` with no
     attached `.output`) — same name is fine, Svelte scopes each
     component's `<style>` independently. */
  @keyframes edgeflash {
    0%,
    100% {
      border-color: color-mix(in srgb, var(--orb) 45%, transparent);
      box-shadow: 0 0 0 0 transparent;
    }
    50% {
      border-color: color-mix(in srgb, var(--orb) 90%, transparent);
      box-shadow: 0 0 20px color-mix(in srgb, var(--orb) 22%, transparent);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .loopwrap.hasout.running {
      animation: none;
    }
  }
</style>
