<!--
  StackCard — one loop in the stack: runtag, alias chip, iteration bar,
  hide-inactive summary lines, cardbar (iteration pill + facet popovers +
  config drawer toggle + duplicate/drag/delete), and the config drawer
  itself. All mutation goes through `stores/stack.ts` ops — this component
  holds no data state of its own, only the ephemeral `cfgOpen` UI toggle and
  drag-hover visuals.
-->
<script lang="ts">
  import { tick } from 'svelte';
  import {
    type StackCard as StackCardT,
    guardActive,
    evalActive,
    configActive,
    guardSummary,
    evalsSummary,
    scheduleSummary,
    maxxSummary,
    configSummary,
    cardIterationsLabel,
    stepCardIterations,
    loopCountTier,
    draftIsHot,
    duplicateInPane,
    removeFromPane,
    updateCardInPane,
    updateDraftInPane,
    commitDraft,
    reorderInPaneRelative,
    aliasAutocomplete,
    resolvePresetAlias,
    applyPreset,
    CARD_COMMANDS,
    commandAutocomplete,
    commandValueAutocomplete,
    detectPendingCommand,
    evalSuiteOptions,
    applySuite,
    EVAL_SUITES,
    type CommandValueSuggestion
  } from '$lib/stores/stack';
  import { repoAutocomplete, repoLabelForPath } from '$lib/stores/repoMenu';
  import { MODEL_OPTIONS, EFFORT_OPTIONS } from '$lib/stores/options';
  import { AUTONOMY_OPTIONS, type StackDefaults } from '$lib/stores/stackDefaults';
  import { branchesByRepo, branchOptionsFor, ensureBranches } from '$lib/stores/branches';
  import type { Option } from '$lib/stores/controls';
  import { runs, bumpCard, bumpUiState } from '$lib/stores/stackRun';
  import { agents } from '$lib/stores/agents';
  import { ICONS, PRESET_ACCENT } from './icons';
  import { dragging } from './dnd';
  import { autoGrow } from './autoGrow';
  import Popover, { togglePopover } from './Popover.svelte';
  import SchedulePopover from './SchedulePopover.svelte';
  import MaxxPopover from './MaxxPopover.svelte';
  import GuardrailsPopover from './GuardrailsPopover.svelte';
  import EvalsPopover from './EvalsPopover.svelte';
  import ConfigDrawer from './ConfigDrawer.svelte';
  import ProvenanceChips from './ProvenanceChips.svelte';
  import TemplatesMenu from './TemplatesMenu.svelte';
  import AutocompleteSuggest from './AutocompleteSuggest.svelte';
  import RunStatsPill from './RunStatsPill.svelte';

  export let card: StackCardT;
  export let paneKey: string;
  export let index: number;
  export let paneDefaults: StackDefaults;
  export let repoOptions: Option[] = [];
  /** True when the stack's own schedule or loop-count governs this pane's
   *  cadence (`perLoopScheduleGoverned` — Stack-1's §1 precedence rule) —
   *  this card's own `scheduled` cron never fires independently while it's
   *  true, so its active-looking chrome must say so rather than pretend to
   *  run on its own. */
  export let scheduleGoverned = false;

  $: accent = card.preset ? PRESET_ACCENT[card.preset] : 'var(--konjo-dim2, rgba(245,245,245,.28))';

  let schedBtn: HTMLButtonElement | undefined;
  let maxBtn: HTMLButtonElement | undefined;
  let guardBtn: HTMLButtonElement | undefined;
  let evalBtn: HTMLButtonElement | undefined;
  let cfgOpen = false;
  let summaryExpanded = false;

  $: schedId = `${card.id}:sched`;
  $: maxId = `${card.id}:max`;
  $: guardId = `${card.id}:guard`;
  $: evalId = `${card.id}:eval`;

  // ── draft branch (Creation-Flow-1) ──────────────────────────────────────────
  // The pane's pre-commit draft renders through this same component with a
  // `'draft'` status rather than a forked DraftCard. Its edits route to the
  // pane's `draft` (not a card in `pane.cards`), and its cardbar swaps the
  // dup/drag/delete cluster for a single `+ add` commit button.
  $: isDraft = card.status === 'draft';
  $: hot = isDraft && draftIsHot(card);
  let goalInput: HTMLTextAreaElement | undefined;

  /** Route a card patch to the right store op: the draft edits the pane's
   *  `draft`; a committed card edits itself in `pane.cards`. */
  function writeCard(patch: Partial<StackCardT>): void {
    if (isDraft) updateDraftInPane(paneKey, patch);
    else updateCardInPane(paneKey, card.id, patch);
  }

  /** The committed (non-draft) card's own goal edit — no autocomplete, no
   *  alias/repo/command re-parsing, just a direct text patch; those tokens
   *  only ever apply once, at commit time, via the draft's `onGoalInput`. */
  function onCommittedGoalInput(e: Event): void {
    writeCard({ goal: (e.currentTarget as HTMLTextAreaElement).value });
  }

  function onGoalInput(e: Event): void {
    writeCard({ goal: (e.currentTarget as HTMLTextAreaElement).value });
    aliasDismissed = false;
    repoDismissed = false;
    cmdDismissed = false;
  }

  /** Commit the draft: mints a real card at the top of the stack and a fresh
   *  empty draft, then re-focuses the (now-empty) goal input for rapid entry. */
  function commit(): void {
    if (!hot) return;
    commitDraft(paneKey, repoOptions);
    void tick().then(() => goalInput?.focus());
  }

  // ── alias autocomplete (`:token`) ────────────────────────────────────────
  // While the goal field is still just a bare `:token` (no space yet), offer
  // a filtered list of the built-in preset aliases. Legacy aliases (e.g. the
  // renamed `:ratchet`→`:gain`) never appear as suggestions — only canonical
  // `PRESET_KEYS` — so the autocomplete never steers anyone toward a
  // deprecated token.
  let goalFocused = false;
  let aliasActiveIndex = 0;
  let aliasDismissed = false;

  $: aliasMatches = aliasAutocomplete(card.goal);
  $: showAliasSuggest = isDraft && goalFocused && !aliasDismissed && aliasMatches.length > 0;
  $: if (aliasActiveIndex >= aliasMatches.length) aliasActiveIndex = Math.max(0, aliasMatches.length - 1);

  /** Replace the `:token` being typed with the full canonical alias plus a
   *  trailing space, so the cursor lands ready to type the goal text next —
   *  the suggestion list closes itself since the goal no longer matches
   *  `^:(\S*)$` once the space is there. Also applies the preset's
   *  alias/evals to the draft immediately via `applyPreset` — mirroring
   *  `selectRepo`/`applyCommandValue`, which already write their resolved
   *  facet onto `card`/`card.config` at selection time rather than waiting
   *  for commit. Without this the provenance chip (`card.alias`) never
   *  appeared and the preset's eval suite never attached until commit. */
  function selectAlias(alias: string): void {
    const key = resolvePresetAlias(alias.slice(1));
    const patched = key ? applyPreset(card, key) : card;
    writeCard({ ...patched, goal: `${alias} ` });
    aliasActiveIndex = 0;
    void tick().then(() => goalInput?.focus());
  }

  // ── repo autocomplete (`@token`) ─────────────────────────────────────────
  // Same shape as the alias autocomplete, but for the trailing `@repo` token
  // instead of the leading `:alias` one — matches the composer grammar's
  // `:alias "goal" @repo ×N` order, where `@repo` is typically typed right
  // after the goal text. Independent dismiss/active state from the alias
  // list since the two can never be active at once (mutually exclusive by
  // construction — one requires a `:` prefix, the other a trailing `@`).
  let repoActiveIndex = 0;
  let repoDismissed = false;

  $: repoMatches = repoAutocomplete(card.goal, repoOptions);
  // The provenance chip's label — reverse-looked-up from the resolved path
  // so the chip survives even though `@token` is stripped from the goal text
  // on commit (see `selectRepo`'s doc comment).
  $: cardRepoLabel = card.config.repo ? repoLabelForPath(card.config.repo, repoOptions) : undefined;
  $: showRepoSuggest = isDraft && goalFocused && !repoDismissed && repoMatches.length > 0;
  $: if (repoActiveIndex >= repoMatches.length) repoActiveIndex = Math.max(0, repoMatches.length - 1);

  /** Replace the trailing `@token` with the full `@owner/name` token plus a
   *  trailing space (keeps the human-readable label visible while typing).
   *  Also writes the *resolved path* straight onto `card.config.repo` —
   *  never relies on `parseComposerInput` re-deriving it from the label text
   *  later, which is the mismatch that made the repo dropdown silently show
   *  "auto" (`options.find(o => o.value === value)` can't match a label
   *  against a path-keyed catalog). The match is always anchored at the end
   *  of the string (`repoAutocomplete` only ever matches the last word), so
   *  "replace the match" and "replace the string's tail" are the same
   *  slice-and-append — no cursor-position tracking needed. */
  function selectRepo(token: string): void {
    const m = /(^|\s)@(\S*)$/.exec(card.goal);
    if (!m) return;
    const suggestion = repoMatches.find((s) => s.token === token);
    writeCard({
      goal: `${card.goal.slice(0, m.index)}${m[1]}${token} `,
      config: { ...card.config, repo: suggestion?.value ?? card.config.repo }
    });
    repoActiveIndex = 0;
    void tick().then(() => goalInput?.focus());
  }

  // ── inline `/command` autocomplete (model/effort/branch/autonomy/eval/
  //    guard/schedule/maxx) ────────────────────────────────────────────────
  // Two-level grammar, mirroring the user's own suggested `/model/<value>`
  // syntax: typing `/` suggests command names (`commandAutocomplete`); picking
  // a value-picker command (model/effort/branch/autonomy/eval) moves into a
  // second `/command/value` token (`commandValueAutocomplete`) against that
  // command's own catalog. Picking a non-value-picker command (guard/
  // schedule/maxx) fires immediately — strips the token and opens the
  // existing popover for it, same as clicking its cardbar icon.
  let cmdActiveIndex = 0;
  let cmdDismissed = false;
  /** Set once a value-picker command is chosen from the level-1 list; cleared
   *  on selection, dismissal, or whenever the goal text changes out from
   *  under it (`onGoalInput`/`onChange` below). */
  let pendingCommand: string | null = null;

  // This card's own repo — not the pane's — drives its branch list, same
  // resolution `ConfigDrawer` uses.
  $: effectiveRepo = card.config.repo ?? paneDefaults.repo;
  $: void ensureBranches(effectiveRepo);

  function commandOptionsFor(command: string): Option[] {
    switch (command) {
      case 'model':
        return MODEL_OPTIONS;
      case 'effort':
        return EFFORT_OPTIONS;
      case 'autonomy':
        return AUTONOMY_OPTIONS;
      case 'branch':
        return branchOptionsFor($branchesByRepo, effectiveRepo);
      case 'eval':
        return evalSuiteOptions();
      default:
        return [];
    }
  }

  $: cmdMatches = pendingCommand
    ? commandValueAutocomplete(card.goal, pendingCommand, commandOptionsFor(pendingCommand))
    : commandAutocomplete(card.goal, CARD_COMMANDS);
  $: showCmdSuggest = isDraft && goalFocused && !cmdDismissed && cmdMatches.length > 0;
  $: if (cmdActiveIndex >= cmdMatches.length) cmdActiveIndex = Math.max(0, cmdMatches.length - 1);
  // Re-infer `pendingCommand` from the goal text on every change, not just
  // from `selectCommand`'s explicit assignment — otherwise hand-typing
  // `/model/` (rather than clicking the `/model` row) never entered
  // value-picker mode. Falls back to the old clear-on-abandon behavior once
  // the `/command/` prefix itself is edited away (e.g. backspaced).
  $: {
    const inferred = detectPendingCommand(card.goal, CARD_COMMANDS);
    if (inferred) {
      pendingCommand = inferred;
    } else if (pendingCommand && !new RegExp(`(^|\\s)/${pendingCommand}/`).test(card.goal)) {
      pendingCommand = null;
    }
  }

  /** Apply a value-picker command's chosen value directly to `card.config`
   *  (or toggle the eval suite) and strip the resolved token from the goal
   *  text — no chip; the existing config-gear/evals-count indicators already
   *  surface these once set. */
  function applyCommandValue(command: string, value: string): void {
    switch (command) {
      case 'eval':
        writeCard({ evals: applySuite(card.evals, EVAL_SUITES[value] ?? []) });
        return;
      case 'model':
        writeCard({ config: { ...card.config, model: value } });
        return;
      case 'effort':
        writeCard({ config: { ...card.config, effort: value } });
        return;
      case 'branch':
        writeCard({ config: { ...card.config, branch: value } });
        return;
      case 'autonomy':
        writeCard({ config: { ...card.config, autonomy: value } });
        return;
    }
  }

  /** Fire a non-value-picker command's immediate action — opens the same
   *  popover its cardbar icon does. */
  function fireCommandAction(command: string): void {
    if (command === 'guard') togglePopover(guardId);
    else if (command === 'schedule') togglePopover(schedId);
    else if (command === 'maxx') togglePopover(maxId);
  }

  function selectCommand(token: string): void {
    if (pendingCommand) {
      const valueMatches = cmdMatches as CommandValueSuggestion[];
      const suggestion = valueMatches.find((s) => s.token === token);
      const m = new RegExp(`(^|\\s)/${pendingCommand}/(\\S*)$`).exec(card.goal);
      if (m && suggestion) {
        writeCard({ goal: `${card.goal.slice(0, m.index)}${m[1]}` });
        applyCommandValue(pendingCommand, suggestion.value);
      }
      pendingCommand = null;
    } else {
      const command = token.slice(1);
      const def = CARD_COMMANDS.find((c) => c.command === command);
      const m = /(^|\s)\/(\S*)$/.exec(card.goal);
      if (!m) return;
      if (def?.isValuePicker) {
        writeCard({ goal: `${card.goal.slice(0, m.index)}${m[1]}/${command}/` });
        pendingCommand = command;
      } else {
        writeCard({ goal: `${card.goal.slice(0, m.index)}${m[1]}` });
        fireCommandAction(command);
      }
    }
    cmdActiveIndex = 0;
    void tick().then(() => goalInput?.focus());
  }

  // ── grammar chips (always-visible entry points into the autocomplete
  //    above) ────────────────────────────────────────────────────────────
  // Each chip inserts the same trigger token a user would type by hand, then
  // hands off to the exact selection path that trigger already opens — no
  // new parsing/selection logic, just a discoverable shortcut into it.
  function chipSpacer(text: string): string {
    return text.length > 0 && !/\s$/.test(text) ? ' ' : '';
  }

  async function chipAlias(): Promise<void> {
    goalFocused = true;
    aliasDismissed = false;
    writeCard({ goal: `${card.goal}${chipSpacer(card.goal)}:` });
    await tick();
    goalInput?.focus();
  }

  async function chipRepo(): Promise<void> {
    goalFocused = true;
    repoDismissed = false;
    writeCard({ goal: `${card.goal}${chipSpacer(card.goal)}@` });
    await tick();
    goalInput?.focus();
  }

  async function chipCommand(command: string): Promise<void> {
    goalFocused = true;
    cmdDismissed = false;
    writeCard({ goal: `${card.goal}${chipSpacer(card.goal)}/` });
    await tick();
    selectCommand(`/${command}`);
  }

  async function chipLoop(): Promise<void> {
    goalFocused = true;
    writeCard({ goal: `${card.goal}${chipSpacer(card.goal)}x3 ` });
    await tick();
    goalInput?.focus();
  }

  function onGoalKeydown(e: KeyboardEvent): void {
    if (showAliasSuggest) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        aliasActiveIndex = (aliasActiveIndex + 1) % aliasMatches.length;
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        aliasActiveIndex = (aliasActiveIndex - 1 + aliasMatches.length) % aliasMatches.length;
        return;
      }
      if (e.key === 'Tab' || e.key === 'Enter') {
        e.preventDefault();
        selectAlias(aliasMatches[aliasActiveIndex].alias);
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        aliasDismissed = true;
        return;
      }
    }
    if (showRepoSuggest) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        repoActiveIndex = (repoActiveIndex + 1) % repoMatches.length;
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        repoActiveIndex = (repoActiveIndex - 1 + repoMatches.length) % repoMatches.length;
        return;
      }
      if (e.key === 'Tab' || e.key === 'Enter') {
        e.preventDefault();
        selectRepo(repoMatches[repoActiveIndex].token);
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        repoDismissed = true;
        return;
      }
    }
    if (showCmdSuggest) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        cmdActiveIndex = (cmdActiveIndex + 1) % cmdMatches.length;
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        cmdActiveIndex = (cmdActiveIndex - 1 + cmdMatches.length) % cmdMatches.length;
        return;
      }
      if (e.key === 'Tab' || e.key === 'Enter') {
        e.preventDefault();
        selectCommand(cmdMatches[cmdActiveIndex].token);
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        cmdDismissed = true;
        return;
      }
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      commit();
    }
  }

  $: guardsOn = guardActive(card.guardrails);
  $: evalsOn = evalActive(card);
  $: configOn = configActive(card, paneDefaults);
  $: scheduleActive = card.scheduled && !scheduleGoverned;
  // The config drawer already shows every field inline while open — the
  // hide-inactive summary line only needs to cover the gap left when it's
  // collapsed (previously nothing surfaced an override at all once closed).
  $: showConfigSummary = configOn && !cfgOpen;
  $: showSep = card.scheduled || card.maxx.enabled || guardsOn || evalsOn || showConfigSummary;
  $: summaryCount = [card.scheduled, card.maxx.enabled, guardsOn, evalsOn, showConfigSummary].filter(Boolean).length;
  // A card's loop reads as "actively running" only once it has both a live
  // iteration (status === 'running') and an actual repeat configured — an
  // off card (single pass) never shows the running-loop chrome even mid-run.
  $: loopRunning = card.status === 'running' && !!card.iteration && card.iteration.total > 1;

  // ×N loop-count color ramp (round 2, item 5) — `null` while off, since the
  // off pill keeps its own neutral `.off` styling untouched by the ramp.
  $: iterTier = card.maxIterations === 0 ? null : loopCountTier(card.maxIterations);

  // Live elapsed/token/cost readout while this card's task is actually
  // running — `AgentState` already ticks `elapsedMs` and accumulates
  // tokens/cost from the wire (see `stores/agents.ts`), so this is a plain
  // lookup, not new accumulation logic.
  $: liveAgent = card.taskId ? $agents.get(card.taskId) : undefined;
  $: showRunStats = card.status === 'running' && !!liveAgent;

  /** Persist the popover's toggle outcome onto the card — independent of
   *  `scheduled`/`cron`; a card can have both on at once. */
  function onMaxxToggled(next: { enabled: boolean; entryId: string | undefined }): void {
    writeCard({ maxx: { ...card.maxx, enabled: next.enabled }, maxxEntryId: next.entryId });
  }

  // The card's running/queued/done border color comes from `--orb`, a CSS
  // custom property set by the parent (`StackPane.svelte`) on the shared
  // `.loopwrap` ancestor rather than computed here — the live-output panel
  // (`StackOutput.svelte`) is a *sibling*, not a descendant, of this card, so
  // for both to inherit the identical value (and stay in visual lockstep)
  // it has to live above both of them, not on this component's own root.
  // The status runtag badge text (mockup's `statusLabel`): a running card
  // reads "running · iter N/M", every other status reads its own name.
  $: statusLabel =
    card.status === 'running' && card.iteration
      ? `running · iter ${card.iteration.current}/${card.iteration.total}`
      : card.status;

  function step(delta: number) {
    writeCard({ maxIterations: stepCardIterations(card.maxIterations, delta) });
  }

  function dupCard() {
    duplicateInPane(paneKey, card.id);
  }
  function delCard() {
    removeFromPane(paneKey, card.id);
  }

  // ── drag to reorder (within this pane only) ─────────────────────────────────
  let dropBefore = false;
  let dropAfter = false;
  let draggable = false;

  function armDrag() {
    draggable = true;
  }
  function disarmDrag() {
    draggable = false;
  }

  // ── mid-run reorder (Backend-1's `bumpCard`, previously wired to no UI) ──────
  // Drag-to-reorder above edits `pane.cards` directly, but `runStack` snapshots
  // its own `order`/`cursor` at launch so a composer edit can't reshuffle a plan
  // already in flight (see `stackRun.ts`'s doc comment) — during an active run,
  // only `bumpCard` actually moves a still-queued card's real turn. `bumpUiState`
  // is the pure predicate (unit-tested in `stackRun.test.ts`) that decides
  // visibility and per-direction enablement so this component stays a thin view.
  $: bumpState = isDraft ? { visible: false, canSooner: false, canLater: false } : bumpUiState($runs.get(paneKey), card.id);

  function bump(direction: 'up' | 'down') {
    bumpCard(paneKey, card.id, direction);
  }
  function onDragStart(e: DragEvent) {
    if (isDraft) return; // the draft is not in pane.cards — never draggable
    dragging.set({ paneKey, cardId: card.id, index });
    if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
  }
  function onDragEnd() {
    dragging.set(null);
    dropBefore = false;
    dropAfter = false;
    draggable = false;
  }
  function onDragOver(e: DragEvent) {
    if (isDraft) return; // never a drop target — reorder must not see the draft
    const cur = $dragging;
    if (!cur || cur.paneKey !== paneKey || cur.cardId === card.id) return;
    e.preventDefault();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const before = e.clientY - rect.top < rect.height / 2;
    dropBefore = before;
    dropAfter = !before;
  }
  function onDragLeave() {
    dropBefore = false;
    dropAfter = false;
  }
  function onDrop(e: DragEvent) {
    if (isDraft) return; // never a drop target — reorder must not see the draft
    e.preventDefault();
    const cur = $dragging;
    const before = dropBefore;
    dropBefore = false;
    dropAfter = false;
    if (!cur || cur.paneKey !== paneKey || cur.cardId === card.id) return;
    reorderInPaneRelative(paneKey, cur.index, index, before);
  }
</script>

<div
  class="pc {card.status}"
  class:draft={isDraft}
  class:hot
  class:dragging={draggable && $dragging?.cardId === card.id}
  class:drop-before={dropBefore}
  class:drop-after={dropAfter}
  style="--accent:{accent}"
  role="listitem"
  draggable={!isDraft && draggable}
  on:dragstart={onDragStart}
  on:dragend={onDragEnd}
  on:dragover={onDragOver}
  on:dragleave={onDragLeave}
  on:drop={onDrop}
>
  <span class="runtag {card.status}">{isDraft ? 'new prompt' : statusLabel}</span>

  {#if isDraft}
    <div class="spec draftspec">
      <TemplatesMenu {card} {paneKey} labeled />
      <ProvenanceChips alias={card.alias} tpl={card.tpl} tplKind={card.tplKind} repoLabel={cardRepoLabel} />
    </div>
    <!-- Goal on its own full-width line, a `<textarea>` (not `<input>`) with
         `use:autoGrow` so a long prompt wraps and stays fully visible
         instead of scrolling off sideways in a single line. Still honors
         `:alias @repo ×N` on commit. -->
    <div class="goalwrap">
      <textarea
        class="goalinput"
        bind:this={goalInput}
        value={card.goal}
        on:input={onGoalInput}
        on:keydown={onGoalKeydown}
        on:focus={() => (goalFocused = true)}
        on:blur={() => (goalFocused = false)}
        use:autoGrow
        rows="1"
        placeholder="describe the prompt or goal..."
        spellcheck="false"
      ></textarea>
      {#if showAliasSuggest}
        <AutocompleteSuggest
          anchor={goalInput}
          items={aliasMatches.map((m) => ({ value: m.alias, label: m.label, hint: m.hint }))}
          activeIndex={aliasActiveIndex}
          onSelect={selectAlias}
        />
      {:else if showRepoSuggest}
        <AutocompleteSuggest
          anchor={goalInput}
          items={repoMatches.map((m) => ({ value: m.token, label: m.label, hint: m.hint }))}
          activeIndex={repoActiveIndex}
          onSelect={selectRepo}
        />
      {:else if showCmdSuggest}
        <AutocompleteSuggest
          anchor={goalInput}
          items={cmdMatches.map((m) => ({ value: m.token, label: m.label, hint: m.hint }))}
          activeIndex={cmdActiveIndex}
          onSelect={selectCommand}
        />
      {/if}
    </div>
    <div class="grammarchips">
      <button type="button" class="gchip alias" on:click={chipAlias}>:alias</button>
      <button type="button" class="gchip repo" on:click={chipRepo}>@repo</button>
      <button type="button" class="gchip model" on:click={() => chipCommand('model')}>/model</button>
      <button type="button" class="gchip effort" on:click={() => chipCommand('effort')}>/effort</button>
      <button type="button" class="gchip loop" on:click={chipLoop}>×N</button>
    </div>
  {:else}
    <div class="spec">
      <ProvenanceChips alias={card.alias} tpl={card.tpl} tplKind={card.tplKind} repoLabel={cardRepoLabel} />
      {#if card.status !== 'running'}
        <textarea
          class="md mdinput"
          value={card.goal}
          on:input={onCommittedGoalInput}
          use:autoGrow
          rows="1"
          spellcheck="false"
          aria-label="edit prompt"
        ></textarea>
      {:else}
        <span class="md">"{card.goal}"</span>
      {/if}
    </div>
  {/if}

  {#if card.status === 'blocked' && card.blockReason}
    <div class="blockreason">{@html ICONS.x}{card.blockReason}</div>
  {/if}

  {#if card.status === 'running' && card.iteration}
    <div class="iterbar">
      {#each Array(card.iteration.total) as _, i}
        <i class={i < card.iteration.current - 1 ? 'done' : i === card.iteration.current - 1 ? 'cur' : ''}></i>
      {/each}
    </div>
  {/if}

  {#if showSep}
    <hr class="sep" />
    <button
      type="button"
      class="sumchip"
      on:click={() => (summaryExpanded = !summaryExpanded)}
      aria-expanded={summaryExpanded}
    >
      {summaryCount} configured {@html summaryExpanded ? ICONS.chevup : ICONS.chevdown}
    </button>
    {#if summaryExpanded}
      {#if card.scheduled}
        <div class="sumln sched" class:governed={scheduleGoverned}>
          <span class="rl">{@html ICONS.cron}schedule</span>
          <span class="txt">
            {#if scheduleGoverned}
              governed by stack — won't fire on its own
            {:else}
              <b>{scheduleSummary(card)}</b>
            {/if}
          </span>
        </div>
      {/if}
      {#if card.maxx.enabled}
        <div class="sumln max">
          <span class="rl">{@html ICONS.bolt}MAXX</span>
          <span class="txt">on{#if maxxSummary(card)} · <b>{maxxSummary(card)}</b>{/if}</span>
        </div>
      {/if}
      {#if guardsOn}
        <div class="sumln guard">
          <span class="rl">{@html ICONS.shield}guards</span>
          <span class="txt">{guardSummary(card)}</span>
        </div>
      {/if}
      {#if evalsOn}
        <div class="sumln eval">
          <span class="rl">{@html ICONS.checkbox}evals</span>
          <span class="txt">{evalsSummary(card)}</span>
        </div>
      {/if}
      {#if showConfigSummary}
        <div class="sumln cfg">
          <span class="rl">{@html ICONS.sliders}config</span>
          <span class="txt">{configSummary(card, paneDefaults)}</span>
        </div>
      {/if}
    {/if}
  {/if}

  <div class="cardbar">
    <span
      class="iterpill"
      class:off={card.maxIterations === 0}
      class:running={loopRunning}
      class:tier-yellow={iterTier === 'yellow'}
      class:tier-red={iterTier === 'red'}
      title={loopRunning
        ? `iteration ${card.iteration?.current}/${card.iteration?.total}`
        : card.maxIterations === 0
          ? 'off · runs once, no repeat'
          : undefined}
    >
      <span class="lb"
        >{@html loopRunning ? ICONS.spinner : ICONS.loop}<span class="val"
          >{loopRunning
            ? `${card.iteration?.current}/${card.iteration?.total}`
            : card.maxIterations === 0
              ? 'off'
              : '×' + cardIterationsLabel(card.maxIterations)}</span
        ></span
      >
      <span class="steppers">
        <button class="sb" on:click={() => step(-1)} title="fewer iterations">−</button>
        <button class="sb" on:click={() => step(1)} title="more iterations">+</button>
      </span>
    </span>
    {#if showRunStats && liveAgent}
      <RunStatsPill
        elapsedMs={liveAgent.elapsedMs}
        tokens={(liveAgent.outputTokens ?? 0) + (liveAgent.inputTokens ?? 0)}
        costUsd={liveAgent.cost}
      />
    {/if}
    <button
      class="ib sched"
      class:act={scheduleActive}
      bind:this={schedBtn}
      on:click={() => togglePopover(schedId)}
      title={scheduleGoverned ? 'schedule (governed by the stack)' : 'schedule'}
    >
      {@html ICONS.cron}
    </button>
    <button
      class="ib guard"
      class:act={guardsOn}
      bind:this={guardBtn}
      on:click={() => togglePopover(guardId)}
      title="guardrails"
    >
      {@html ICONS.shield}
    </button>
    <button
      class="ib eval"
      class:act={evalsOn}
      bind:this={evalBtn}
      on:click={() => togglePopover(evalId)}
      title="evals"
    >
      {@html ICONS.checkbox}<span class="cnt">{card.evals.length}</span>
    </button>
    <button
      class="ib max"
      class:act={card.maxx.enabled}
      bind:this={maxBtn}
      on:click={() => togglePopover(maxId)}
      title="MAXX"
    >
      {@html ICONS.bolt}
    </button>
    <button class="ib config" class:act={configOn} on:click={() => (cfgOpen = !cfgOpen)} title="run config">
      {@html ICONS.sliders}
    </button>
    <span class="sp"></span>
    {#if isDraft}
      <button class="ib add" disabled={!hot} on:click={commit} title="add to stack">
        {@html ICONS.plus}<span class="addlbl">add</span>
      </button>
    {:else}
      <TemplatesMenu {card} {paneKey} />
      {#if bumpState.visible}
        <button
          class="ib bump"
          disabled={!bumpState.canSooner}
          on:click={() => bump('up')}
          title="run sooner — moves this card earlier in the active run's queue"
        >
          {@html ICONS.chevup}
        </button>
        <button
          class="ib bump"
          disabled={!bumpState.canLater}
          on:click={() => bump('down')}
          title="run later — moves this card later in the active run's queue"
        >
          {@html ICONS.chevdown}
        </button>
      {/if}
      <button class="ib" on:click={dupCard} title="duplicate">{@html ICONS.dup}</button>
      <button
        class="ib drag"
        title="drag to reorder"
        on:mousedown={armDrag}
        on:mouseup={disarmDrag}
      >
        {@html ICONS.drag}
      </button>
      <button class="ib danger" on:click={delCard} title="delete">{@html ICONS.trash}</button>
    {/if}
  </div>

  {#if cfgOpen}
    <ConfigDrawer {card} {paneKey} {paneDefaults} {repoOptions} onWrite={writeCard} />
  {/if}
</div>

<Popover id={schedId} anchor={schedBtn ?? null} kind="sched">
  <SchedulePopover
    scheduled={card.scheduled}
    cron={card.cron}
    onToggle={() => writeCard({ scheduled: !card.scheduled })}
    onChange={(next) => writeCard({ cron: next })}
  />
</Popover>
<Popover id={maxId} anchor={maxBtn ?? null} kind="max">
  <MaxxPopover
    maxx={card.maxx}
    entryId={card.maxxEntryId}
    goal={card.goal}
    repo={card.config.repo}
    onToggled={onMaxxToggled}
  />
</Popover>
<Popover id={guardId} anchor={guardBtn ?? null} kind="guard">
  <GuardrailsPopover
    scope="loop"
    gate={card.guardrails.gate}
    gateCmd={card.guardrails.gateCmd}
    until={card.guardrails.until}
    untilCmd={card.guardrails.untilCmd}
    onFail={card.guardrails.onFail}
    budget={card.guardrails.budget}
    onChangeGate={(patch) => writeCard({ guardrails: { ...card.guardrails, ...patch } })}
    onChangeUntil={(patch) => writeCard({ guardrails: { ...card.guardrails, ...patch } })}
    onChangeOnFail={(onFail) => writeCard({ guardrails: { ...card.guardrails, onFail } })}
    onChangeBudget={(budget) => writeCard({ guardrails: { ...card.guardrails, budget } })}
    maxIterations={card.maxIterations}
    onStep={step}
  />
</Popover>
<Popover id={evalId} anchor={evalBtn ?? null} kind="eval">
  <EvalsPopover evals={card.evals} onChange={(evals) => writeCard({ evals })} />
</Popover>

<style>
  .pc {
    position: relative;
    background: var(--konjo-card, #0e1214);
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: 9px;
    padding: 13px 14px;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.08),
      0 1px 2px rgba(0, 0, 0, 0.4);
    transition:
      box-shadow 0.12s,
      border-color 0.12s;
  }
  .pc.running {
    border-color: color-mix(in srgb, var(--orb) 45%, transparent);
    animation: edgeflash 5s ease-in-out infinite;
  }
  .pc.queued {
    border-color: color-mix(in srgb, var(--orb) 40%, transparent);
  }
  .pc.done {
    border-color: color-mix(in srgb, var(--orb) 35%, transparent);
  }
  /* Blocked/error (round 2, item 3) — rose, static (no edgeflash; a blocked
     run is terminal, not actively in motion). Fixed rose rather than
     `--orb`-derived like `.pc.done`/`.queued`/`.running`: `card.status` is
     the pane's own durable state, while `--orb` is a live lookup keyed by
     `taskId` into the `agents` store — one that goes stale/empty on reload
     long before the card itself stops reading `'blocked'`. */
  .pc.blocked {
    border-color: rgba(255, 0, 102, 0.45);
  }
  /* Draft card (Creation-Flow-1): dashed until it carries content, then a
     teal "hot" border signalling it's ready to commit. */
  .pc.draft {
    border-style: dashed;
    border-color: rgba(255, 255, 255, 0.18);
  }
  .pc.draft.hot {
    border-style: solid;
    border-color: rgba(0, 255, 212, 0.5);
    box-shadow: 0 0 18px rgba(0, 255, 212, 0.08);
  }
  .runtag.draft {
    color: rgba(245, 245, 245, 0.46);
  }
  .pc.draft.hot .runtag.draft {
    color: var(--stack-teal, #00ffd4);
    border-color: rgba(0, 255, 212, 0.45);
  }
  .draftspec {
    row-gap: 7px;
  }
  .goalwrap {
    position: relative;
    margin-top: 10px;
  }
  .goalinput {
    display: block;
    width: 100%;
    box-sizing: border-box;
    resize: none;
    overflow: hidden;
    background: rgba(255, 255, 255, 0.02);
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 7px;
    padding: 9px 11px;
    color: var(--konjo-paper, #f5f5f5);
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    font-size: 14px;
    line-height: 1.5;
    outline: none;
    transition:
      border-color 0.12s,
      background 0.12s;
  }
  .goalinput::placeholder {
    color: rgba(245, 245, 245, 0.28);
  }
  .goalinput:focus {
    border-color: rgba(0, 255, 212, 0.4);
    background: rgba(0, 255, 212, 0.03);
  }
  .grammarchips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 8px;
  }
  .gchip {
    height: 22px;
    display: inline-flex;
    align-items: center;
    padding: 0 8px;
    border-radius: 11px;
    background: transparent;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    font-size: 9.5px;
    cursor: pointer;
    transition: 0.12s;
  }
  .gchip.alias {
    border: 1px solid rgba(0, 255, 212, 0.4);
    color: var(--stack-teal, #00ffd4);
  }
  .gchip.alias:hover {
    border-color: rgba(0, 255, 212, 0.7);
    background: rgba(0, 255, 212, 0.08);
  }
  .gchip.repo {
    border: 1px solid rgba(0, 212, 255, 0.4);
    color: var(--konjo-ice, #00d4ff);
  }
  .gchip.repo:hover {
    border-color: rgba(0, 212, 255, 0.7);
    background: rgba(0, 212, 255, 0.08);
  }
  .gchip.model {
    border: 1px solid rgba(183, 155, 255, 0.4);
    color: var(--stack-violet, #b79bff);
  }
  .gchip.model:hover {
    border-color: rgba(183, 155, 255, 0.7);
    background: rgba(183, 155, 255, 0.08);
  }
  .gchip.effort {
    border: 1px solid rgba(255, 149, 0, 0.4);
    color: var(--konjo-flame, #ff9500);
  }
  .gchip.effort:hover {
    border-color: rgba(255, 149, 0, 0.7);
    background: rgba(255, 149, 0, 0.08);
  }
  .gchip.loop {
    border: 1px solid rgba(255, 204, 0, 0.4);
    color: var(--konjo-sun, #ffcc00);
  }
  .gchip.loop:hover {
    border-color: rgba(255, 204, 0, 0.7);
    background: rgba(255, 204, 0, 0.08);
  }
  .ib.add {
    color: var(--konjo-jade, #00ff9d);
    border-color: rgba(0, 255, 157, 0.5);
    background: rgba(0, 255, 157, 0.08);
    font-weight: 700;
    padding: 0 12px;
  }
  .ib.add .addlbl {
    font-size: 11px;
  }
  .ib.add:hover:not(:disabled) {
    color: var(--konjo-jade, #00ff9d);
    border-color: rgba(0, 255, 157, 0.8);
    background: rgba(0, 255, 157, 0.14);
  }
  .ib.add:disabled {
    opacity: 0.4;
    cursor: not-allowed;
    color: rgba(245, 245, 245, 0.28);
    border-color: rgba(255, 255, 255, 0.11);
    background: transparent;
  }
  .pc.dragging {
    opacity: 0.4;
  }
  .pc.drop-before {
    box-shadow: 0 -3px 0 var(--konjo-ice);
  }
  .pc.drop-after {
    box-shadow: 0 3px 0 var(--konjo-ice);
  }
  /* Only actually paints when this card has no output attached — StackPane's
     `.loopwrap.hasout` strips `.pc`'s border (`border: none !important`) and
     takes over the identical animation itself once a `taskId` exists, since
     two separately-animated elements can share this exact color/keyframes
     and still drift out of phase (each one's `animation` clocks from its
     own mount time, not a shared clock). Kept as a real fallback here, not
     dead code, for a running card that somehow has no `taskId` yet. */
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
  /* Status runtag badge, sitting in a notch on the card's top edge — the
     mockup's `.runtag`. Colour + a pulsing dot (running) read the card status. */
  .runtag {
    position: absolute;
    top: -10px;
    right: 14px;
    font-size: 9px;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    background: var(--konjo-black, #0b0e10);
    border: 1px solid rgba(255, 255, 255, 0.11);
    border-radius: 3px;
    padding: 2px 8px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: rgba(245, 245, 245, 0.46);
    z-index: 2;
  }
  .runtag.running {
    color: var(--konjo-flame, #ff9500);
    border-color: rgba(255, 149, 0, 0.5);
  }
  .runtag.running::before {
    content: '';
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--konjo-flame, #ff9500);
    box-shadow: 0 0 5px var(--konjo-ember, #ff4500);
    animation: pulse 1.4s infinite;
  }
  .runtag.queued {
    color: var(--konjo-ice, #00d4ff);
    border-color: rgba(0, 212, 255, 0.45);
  }
  .runtag.done {
    color: var(--konjo-jade, #00ff9d);
    border-color: rgba(0, 255, 157, 0.45);
  }
  .runtag.blocked {
    color: var(--konjo-rose, #ff0066);
    border-color: rgba(255, 0, 102, 0.5);
  }
  /* Blocked-run inline reason (round 2, item 3) — only rendered when the
     card actually carries a failure message, immediately under the goal
     text. */
  .blockreason {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 9px;
    padding: 8px 10px;
    border-radius: 7px;
    background: rgba(255, 0, 102, 0.08);
    color: #ffaacb;
    font-size: 10px;
    line-height: 1.4;
  }
  .blockreason :global(svg) {
    width: 12px;
    height: 12px;
    flex: 0 0 auto;
    color: var(--konjo-rose, #ff0066);
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
  .spec .md {
    color: rgba(245, 245, 245, 0.46);
  }
  /* Committed cards' goal is editable (as long as the card isn't running) —
     styled to read as plain text at rest and reveal an input affordance on
     hover/focus, rather than looking like a form field all the time.
     `<textarea>`, not `<input>`, so a long prompt wraps and stays fully
     visible (the auto-grow action above sizes it to content) instead of
     scrolling off sideways in a single line. */
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
    color: rgba(245, 245, 245, 0.46);
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
    border-color: rgba(255, 255, 255, 0.11);
    background: rgba(255, 255, 255, 0.02);
  }
  .spec .mdinput:focus {
    border-color: rgba(0, 255, 212, 0.4);
    background: rgba(0, 255, 212, 0.03);
    color: var(--konjo-paper, #f5f5f5);
  }
  .iterbar {
    display: flex;
    gap: 4px;
    margin-top: 9px;
  }
  .iterbar i {
    height: 3px;
    width: 22px;
    border-radius: 2px;
    background: rgba(255, 255, 255, 0.11);
  }
  .iterbar i.done {
    background: var(--konjo-jade);
  }
  .iterbar i.cur {
    background: var(--konjo-flame);
    box-shadow: 0 0 5px var(--konjo-ember);
    animation: pulse 1.8s infinite;
  }
  @keyframes pulse {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.5;
    }
  }
  .sep {
    height: 1px;
    background: rgba(255, 255, 255, 0.05);
    border: none;
    margin-top: 11px;
  }
  .sumchip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 24px;
    margin-top: 9px;
    padding: 0 10px;
    border-radius: 12px;
    border: 1px solid rgba(255, 255, 255, 0.16);
    background: rgba(255, 255, 255, 0.04);
    color: rgba(245, 245, 245, 0.7);
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    font-size: 10px;
    cursor: pointer;
    transition: 0.12s;
  }
  .sumchip:hover {
    border-color: rgba(255, 255, 255, 0.32);
    background: rgba(255, 255, 255, 0.08);
  }
  .sumchip :global(svg) {
    width: 11px;
    height: 11px;
  }
  .sumln {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-top: 8px;
    font-size: 9.5px;
    min-width: 0;
  }
  .sumln .rl {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    font-size: 8px;
    flex: 0 0 auto;
    width: 64px;
  }
  .sumln .rl :global(svg) {
    width: 11px;
    height: 11px;
  }
  .sumln.sched .rl {
    color: rgba(245, 245, 245, 0.6);
  }
  .sumln.max .rl {
    color: rgba(245, 245, 245, 0.6);
  }
  .sumln.guard .rl {
    color: rgba(245, 245, 245, 0.6);
  }
  .sumln.eval .rl {
    color: rgba(245, 245, 245, 0.6);
  }
  .sumln.cfg .rl {
    color: rgba(245, 245, 245, 0.6);
  }
  .sumln .txt {
    color: rgba(245, 245, 245, 0.46);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1;
    min-width: 0;
  }
  .sumln.sched .txt b {
    color: var(--konjo-ice);
  }
  .sumln.sched.governed .rl {
    color: rgba(245, 245, 245, 0.28);
  }
  .sumln.max .txt b {
    color: var(--konjo-flame);
  }
  .cardbar {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 12px;
  }
  .ib {
    position: relative;
    height: 29px;
    min-width: 29px;
    padding: 0 7px;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.11);
    background: transparent;
    color: rgba(245, 245, 245, 0.28);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    font-size: 11px;
    transition: 0.12s;
  }
  .ib :global(svg) {
    width: 14px;
    height: 14px;
  }
  .ib:hover {
    color: var(--konjo-paper, #f5f5f5);
    border-color: rgba(245, 245, 245, 0.46);
  }
  .ib .cnt {
    font-size: 9px;
    font-weight: 700;
  }
  .ib.sched.act {
    color: #f5f5f5;
    border-color: rgba(255, 255, 255, 0.5);
    background: rgba(255, 255, 255, 0.1);
  }
  .ib.max.act {
    color: #f5f5f5;
    border-color: rgba(255, 255, 255, 0.5);
    background: rgba(255, 255, 255, 0.1);
  }
  .ib.danger:hover {
    color: var(--konjo-rose, #ff0066);
    border-color: rgba(255, 0, 102, 0.4);
  }
  .ib.guard.act {
    color: #f5f5f5;
    border-color: rgba(255, 255, 255, 0.5);
    background: rgba(255, 255, 255, 0.1);
  }
  .ib.eval.act {
    color: #f5f5f5;
    border-color: rgba(255, 255, 255, 0.5);
    background: rgba(255, 255, 255, 0.1);
  }
  .ib.config.act {
    color: #f5f5f5;
    border-color: rgba(255, 255, 255, 0.5);
    background: rgba(255, 255, 255, 0.1);
  }
  .ib.drag {
    cursor: grab;
  }
  .ib.drag:active {
    cursor: grabbing;
  }
  .ib.bump {
    padding: 0 5px;
  }
  .ib.bump:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }
  .sp {
    flex: 1;
  }
  .iterpill {
    display: inline-flex;
    align-items: center;
    height: 29px;
    border: 1px solid rgba(255, 149, 0, 0.5);
    background: rgba(255, 69, 0, 0.09);
    border-radius: 6px;
    overflow: hidden;
    font-size: 11px;
    color: var(--konjo-flame);
    font-weight: 700;
  }
  .iterpill .lb {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 0 9px;
    height: 100%;
  }
  .iterpill .lb :global(svg) {
    width: 14px;
    height: 14px;
  }
  .iterpill .steppers {
    display: inline-flex;
    align-items: center;
    max-width: 0;
    overflow: hidden;
    transition: max-width 0.24s cubic-bezier(0.5, 0, 0.2, 1);
  }
  .iterpill:hover .steppers,
  .iterpill:focus-within .steppers {
    max-width: 64px;
  }
  .iterpill .sb {
    width: 28px;
    height: 29px;
    border: none;
    border-left: 1px solid rgba(255, 149, 0, 0.35);
    background: transparent;
    color: var(--konjo-flame);
    font-size: 15px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .iterpill .sb:hover {
    background: rgba(255, 149, 0, 0.2);
  }
  .iterpill.off {
    border-color: rgba(245, 245, 245, 0.22);
    background: rgba(245, 245, 245, 0.05);
    color: rgba(245, 245, 245, 0.4);
  }
  .iterpill.off .sb {
    border-left-color: rgba(245, 245, 245, 0.16);
    color: rgba(245, 245, 245, 0.4);
  }
  .iterpill.off .sb:hover {
    background: rgba(245, 245, 245, 0.08);
  }
  /* ×N color ramp (round 2, item 5) — untagged pill stays the pre-ramp
     orange baseline; these two classes are the only overrides needed. */
  .iterpill.tier-yellow {
    border-color: rgba(255, 204, 0, 0.5);
    background: rgba(255, 204, 0, 0.08);
    color: #ffcc00;
  }
  .iterpill.tier-yellow .sb {
    border-left-color: rgba(255, 204, 0, 0.35);
    color: #ffcc00;
  }
  .iterpill.tier-yellow .sb:hover {
    background: rgba(255, 204, 0, 0.2);
  }
  .iterpill.tier-red {
    border-color: rgba(255, 0, 102, 0.5);
    background: rgba(255, 0, 102, 0.1);
    color: #ff0066;
  }
  .iterpill.tier-red .sb {
    border-left-color: rgba(255, 0, 102, 0.35);
    color: #ff0066;
  }
  .iterpill.tier-red .sb:hover {
    background: rgba(255, 0, 102, 0.2);
  }
  /* Running-loop chrome (card.status === 'running' with a real repeat
     configured): a slow glow on the pill itself, distinct from the card's own
     faster `edgeflash` border pulse, plus a continuously-spinning icon so the
     pill reads as actively mid-iteration rather than just "on". */
  .iterpill.running {
    animation: iterglow 2.4s ease-in-out infinite;
  }
  @keyframes iterglow {
    0%,
    100% {
      box-shadow: 0 0 0 0 rgba(255, 149, 0, 0);
      border-color: rgba(255, 149, 0, 0.5);
    }
    50% {
      box-shadow: 0 0 14px 1px rgba(255, 149, 0, 0.45);
      border-color: rgba(255, 149, 0, 0.95);
    }
  }
  .iterpill .lb :global(svg.spin) {
    animation: spin 1.1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .pc.running,
    .iterbar i.cur,
    .iterpill.running,
    .iterpill .lb :global(svg.spin) {
      animation: none;
    }
  }
</style>
