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
    cardGoalActive,
    cardPursuesGoal,
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
    insertCardIntoPane,
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
    tokenizeGoalChips,
    claudeCommandAutocomplete,
    loopAutocomplete,
    type CommandSuggestion,
    type CommandValueSuggestion
  } from '$lib/stores/stack';
  import { repoAutocomplete, repoLabelForPath } from '$lib/stores/repoMenu';
  import { MODEL_OPTIONS, EFFORT_OPTIONS } from '$lib/stores/options';
  import { AUTONOMY_OPTIONS, type StackDefaults } from '$lib/stores/stackDefaults';
  import { branchesByRepo, branchOptionsFor, ensureBranches } from '$lib/stores/branches';
  import { claudeCommandsByRepo, claudeCommandOptionsFor, ensureClaudeCommands } from '$lib/stores/claudeCommands';
  import type { Option } from '$lib/stores/controls';
  import { runs, bumpCard, bumpUiState } from '$lib/stores/stackRun';
  import { agents } from '$lib/stores/agents';
  import { ICONS, PRESET_ACCENT } from './icons';
  import { dragging } from './dnd';
  import { autoGrow } from './autoGrow';
  import { showToast } from '$lib/stores/toastStore';
  import Popover, { togglePopover, activePopoverId } from './Popover.svelte';
  import SchedulePopover from './SchedulePopover.svelte';
  import MaxxPopover from './MaxxPopover.svelte';
  import GuardrailsPopover from './GuardrailsPopover.svelte';
  import EvalsPopover from './EvalsPopover.svelte';
  import GoalPopover from './GoalPopover.svelte';
  import ConfigDrawer from './ConfigDrawer.svelte';
  import ProvenanceChips from './ProvenanceChips.svelte';
  import TemplatesMenu from './TemplatesMenu.svelte';
  import AutocompleteSuggest from './AutocompleteSuggest.svelte';
  import ChipInput from './ChipInput.svelte';
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

  $: accent = card.preset ? PRESET_ACCENT[card.preset] : 'var(--konjo-dim2, rgb(var(--k-text-primary-rgb) / .28))';

  let schedBtn: HTMLButtonElement | undefined;
  let maxBtn: HTMLButtonElement | undefined;
  let guardBtn: HTMLButtonElement | undefined;
  let evalBtn: HTMLButtonElement | undefined;
  let goalBtn: HTMLButtonElement | undefined;
  let overflowBtn: HTMLButtonElement | undefined;
  let cfgOpen = false;
  let summaryExpanded = false;

  $: schedId = `${card.id}:sched`;
  $: maxId = `${card.id}:max`;
  $: guardId = `${card.id}:guard`;
  $: evalId = `${card.id}:eval`;
  $: goalId = `${card.id}:goal`;
  $: overflowId = `${card.id}:overflow`;

  // Claude-Desktop-style running view (UI-3): while a card is actively
  // running, every facet/ops control (templates, copy, drag, delete, config,
  // maxx, goal, evals, guardrails, schedule) and the "N configured" summary
  // collapse behind a single "..." button — only the loop pill (and the live
  // run-stats readout) stay inline. Gated on `card.status` alone, not
  // `loopRunning` below (which also requires a real repeat) — a single-pass
  // run declutters the same way.
  $: isRunning = card.status === 'running';
  // The config drawer (and the "N configured" summary toggle) have no home
  // in the collapsed running view — auto-close rather than leave them
  // floating open beneath a card that's otherwise gone flat.
  $: if (isRunning) {
    cfgOpen = false;
    summaryExpanded = false;
  }

  // ── draft branch (Creation-Flow-1) ──────────────────────────────────────────
  // The pane's pre-commit draft renders through this same component with a
  // `'draft'` status rather than a forked DraftCard. Its edits route to the
  // pane's `draft` (not a card in `pane.cards`), and its cardbar swaps the
  // dup/drag/delete cluster for a single `+ add` commit button.
  $: isDraft = card.status === 'draft';
  $: hot = isDraft && draftIsHot(card);
  // Round 2, item 2 — the draft's own goal field is a `ChipInput`, not a
  // plain `<textarea>`; `goalInput` is now that contenteditable `<div>`
  // (bound out via `bind:rootEl`), not a native textarea element. Every
  // existing `goalInput?.focus()` / `anchor={goalInput}` call site below
  // keeps working unchanged — both `.focus()` and `AutocompleteSuggest`'s
  // `anchor` prop work identically against any `HTMLElement`.
  let goalInput: HTMLDivElement | undefined;

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

  /** `ChipInput`'s `onInput` hands back the plain serialized string directly
   *  (no `Event`/`currentTarget` to unwrap — see `ChipInput.svelte`'s doc
   *  comment on why it owns its own DOM serialization). */
  function onGoalInput(value: string): void {
    writeCard({ goal: value });
    aliasDismissed = false;
    repoDismissed = false;
    cmdDismissed = false;
    claudeDismissed = false;
    loopDismissed = false;
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

  // Round 2, item 2 — resolved-token chip segments for the draft's ChipInput.
  // Pure derivation off `card.goal`; see `tokenizeGoalChips`'s doc comment in
  // stores/stack.ts for why this is a distinct concern from the autocomplete
  // matching just below. `claudeCommandOptions` (declared below, alongside
  // `effectiveRepo`) is in scope here regardless of source order — Svelte
  // resolves `$:` statements by dependency, not declaration position.
  $: goalSegments = tokenizeGoalChips(
    card.goal,
    CARD_COMMANDS,
    claudeCommandOptions.map((o) => o.value)
  );

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

  // ── inline `;command` autocomplete (model/effort/branch/autonomy/eval/
  //    guard/schedule/maxx) ────────────────────────────────────────────────
  // Two-level grammar, mirroring the user's own suggested `/model/<value>`
  // syntax under lopi's own `;` catch-all prefix: typing `;` suggests command
  // names (`commandAutocomplete`); picking a value-picker command (model/
  // effort/branch/autonomy/eval) moves into a second `;command/value` token
  // (`commandValueAutocomplete`) against that command's own catalog. Picking
  // a non-value-picker command (guard/schedule/maxx) fires immediately —
  // strips the token and opens the existing popover for it, same as clicking
  // its cardbar icon.
  let cmdActiveIndex = 0;
  let cmdDismissed = false;
  /** Set once a value-picker command is chosen from the level-1 list; cleared
   *  on selection, dismissal, or whenever the goal text changes out from
   *  under it (`onGoalInput`/`onChange` below). */
  let pendingCommand: string | null = null;

  // ── real Claude Code `/name` command autocomplete (Composer-Grammar-2) ───
  // Single-level, unlike `;command` above — no value-picker step, see
  // `claudeCommandAutocomplete`'s doc comment in stores/stack.ts.
  let claudeActiveIndex = 0;
  let claudeDismissed = false;

  // This card's own repo — not the pane's — drives its branch list, same
  // resolution `ConfigDrawer` uses.
  $: effectiveRepo = card.config.repo ?? paneDefaults.repo;
  $: void ensureBranches(effectiveRepo);
  // Composer-Grammar-2 — same effective-repo resolution drives the real
  // Claude Code `/name` command catalog.
  $: void ensureClaudeCommands(effectiveRepo);
  $: claudeCommandOptions = claudeCommandOptionsFor($claudeCommandsByRepo, effectiveRepo);

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
  // `;model/` (rather than clicking the `;model` row) never entered
  // value-picker mode. Falls back to the old clear-on-abandon behavior once
  // the `;command/` prefix itself is edited away (e.g. backspaced).
  $: {
    const inferred = detectPendingCommand(card.goal, CARD_COMMANDS);
    if (inferred) {
      pendingCommand = inferred;
    } else if (pendingCommand && !new RegExp(`(^|\\s);${pendingCommand}/`).test(card.goal)) {
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

  /** After a `;command/value` selection lands on `card.config`
   *  (`applyCommandValue`), surface the same place pressing the config-gear
   *  button would open — otherwise the only visible feedback was the gear
   *  icon quietly turning "active" (the reported bug: the typed text
   *  vanished and "only the config button is highlighted"). `eval` has no
   *  `ConfigDrawer` field of its own — its home is the evals popover. */
  function revealConfigSurfaceFor(command: string): void {
    if (command === 'eval') activePopoverId.set(evalId);
    else cfgOpen = true;
  }

  function selectCommand(token: string): void {
    if (pendingCommand) {
      const valueMatches = cmdMatches as CommandValueSuggestion[];
      const suggestion = valueMatches.find((s) => s.token === token);
      const m = new RegExp(`(^|\\s);${pendingCommand}/(\\S*)$`).exec(card.goal);
      if (m && suggestion) {
        // Keep the resolved token in the text (plus a trailing space, same
        // convention `selectRepo`/`selectAlias` already use) so it renders as
        // a colored inline chip via `tokenizeGoalChips` instead of silently
        // vanishing — the picked value already lives structurally on
        // `card.config` via `applyCommandValue` below; `parseComposerInput`
        // strips this same token back out at commit time so it never leaks
        // into the real submitted goal.
        writeCard({ goal: `${card.goal.slice(0, m.index)}${m[1]}${suggestion.token} ` });
        applyCommandValue(pendingCommand, suggestion.value);
        revealConfigSurfaceFor(pendingCommand);
      }
      pendingCommand = null;
    } else {
      const command = token.slice(1);
      const def = CARD_COMMANDS.find((c) => c.command === command);
      const m = /(^|\s);(\S*)$/.exec(card.goal);
      if (!m) return;
      if (def?.isValuePicker) {
        writeCard({ goal: `${card.goal.slice(0, m.index)}${m[1]};${command}/` });
        pendingCommand = command;
      } else {
        writeCard({ goal: `${card.goal.slice(0, m.index)}${m[1]}` });
        fireCommandAction(command);
      }
    }
    cmdActiveIndex = 0;
    void tick().then(() => goalInput?.focus());
  }

  $: claudeMatches = claudeCommandAutocomplete(card.goal, claudeCommandOptions);
  $: showClaudeSuggest = isDraft && goalFocused && !claudeDismissed && claudeMatches.length > 0;
  $: if (claudeActiveIndex >= claudeMatches.length) claudeActiveIndex = Math.max(0, claudeMatches.length - 1);

  /** Replace the trailing `/token` being typed with the full `/name` token
   *  plus a trailing space — no config write, unlike `selectRepo`/
   *  `applyCommandValue`: a real Claude command carries no lopi-side facet,
   *  it is passed straight through to `claude -p` as goal text (see
   *  `claude.rs`'s `build_plan_prompt` — Composer-Grammar-2's Phase 3). */
  function selectClaudeCommand(token: string): void {
    const m = /(^|\s)\/(\S*)$/.exec(card.goal);
    if (!m) return;
    writeCard({ goal: `${card.goal.slice(0, m.index)}${m[1]}${token} ` });
    claudeActiveIndex = 0;
    void tick().then(() => goalInput?.focus());
  }

  // ── ×N loop-count autocomplete (`xN`/`XN`/`×N`) ──────────────────────────
  // Same trailing-word shape as the other suggestion lists — offers ×1-×10
  // as soon as an `x`/`X`/`×` trigger character appears, filtered by any
  // digits typed so far. Picking one resolves the same way `x2 ` typed by
  // hand does (see `ChipInput.svelte`'s trailing-space chip resolution) —
  // splices in the full token plus a trailing space.
  let loopActiveIndex = 0;
  let loopDismissed = false;

  $: loopMatches = loopAutocomplete(card.goal);
  $: showLoopSuggest = isDraft && goalFocused && !loopDismissed && loopMatches.length > 0;
  $: if (loopActiveIndex >= loopMatches.length) loopActiveIndex = Math.max(0, loopMatches.length - 1);

  function selectLoop(token: string): void {
    const m = /(^|\s)[×xX](\d*)$/.exec(card.goal);
    if (!m) return;
    writeCard({ goal: `${card.goal.slice(0, m.index)}${m[1]}${token} ` });
    loopActiveIndex = 0;
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
    writeCard({ goal: `${card.goal}${chipSpacer(card.goal)};` });
    await tick();
    selectCommand(`;${command}`);
  }

  async function chipLoop(): Promise<void> {
    goalFocused = true;
    writeCard({ goal: `${card.goal}${chipSpacer(card.goal)}x3 ` });
    await tick();
    goalInput?.focus();
  }

  /** Unlike `chipCommand`, no single command to auto-select — the repo's
   *  catalog is dynamic, so this only opens the level-1 list (mirrors
   *  `chipAlias`/`chipRepo`'s bare-trigger shape, not `chipCommand`'s
   *  immediate level-2 jump). */
  async function chipClaude(): Promise<void> {
    goalFocused = true;
    claudeDismissed = false;
    writeCard({ goal: `${card.goal}${chipSpacer(card.goal)}/` });
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
    if (showClaudeSuggest) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        claudeActiveIndex = (claudeActiveIndex + 1) % claudeMatches.length;
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        claudeActiveIndex = (claudeActiveIndex - 1 + claudeMatches.length) % claudeMatches.length;
        return;
      }
      if (e.key === 'Tab' || e.key === 'Enter') {
        e.preventDefault();
        selectClaudeCommand(claudeMatches[claudeActiveIndex].token);
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        claudeDismissed = true;
        return;
      }
    }
    if (showLoopSuggest) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        loopActiveIndex = (loopActiveIndex + 1) % loopMatches.length;
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        loopActiveIndex = (loopActiveIndex - 1 + loopMatches.length) % loopMatches.length;
        return;
      }
      if (e.key === 'Tab' || e.key === 'Enter') {
        e.preventDefault();
        selectLoop(loopMatches[loopActiveIndex].token);
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        loopDismissed = true;
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
  $: goalOn = cardGoalActive(card);
  $: goalPursues = cardPursuesGoal(card);
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

  // ── running-view overflow menu (UI-3) ───────────────────────────────────
  // Every facet/ops control folds behind this one "..." button while the
  // card is running (see `isRunning` above) — its own buttons are the exact
  // same elements/handlers as the normal cardbar row, just relocated into a
  // small floating panel so their `bind:this` anchors (schedBtn/guardBtn/…)
  // stay mounted and each facet's own `Popover` keeps working unchanged.
  // Deliberately does NOT close on outside-click (unlike `Popover.svelte`):
  // a click inside a facet popover it spawns (e.g. the schedule popover)
  // renders outside this menu's own DOM subtree — `document`-level "outside"
  // detection can't tell that apart from an actual dismiss click. Closes on
  // its own toggle, Escape (global, like `Popover.svelte`'s own — a plain
  // `<div>` keydown handler isn't a valid a11y target for this), or the run
  // finishing.
  let overflowOpen = false;
  $: if (!isRunning) overflowOpen = false;

  function onOverflowKeydown(e: KeyboardEvent): void {
    if (overflowOpen && e.key === 'Escape') overflowOpen = false;
    // A component may only have one `<svelte:window>`, so the ×N dropdown's
    // Escape-to-close rides this same top-level listener rather than
    // mounting a second one of its own.
    if (iterMenuOpen && e.key === 'Escape') iterMenuOpen = false;
  }

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

  // ×N direct-pick dropdown — the steppers are fine for nudging by one, but
  // dialing "off" up to 10 took nine clicks with no other way in. Values
  // 1-10 plus "off" cover the common range directly; anything higher still
  // only reachable via the `+` stepper, which the dropdown doesn't replace.
  const ITER_PICK_VALUES = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
  let iterMenuOpen = false;

  function pickIterations(n: number): void {
    writeCard({ maxIterations: n });
    iterMenuOpen = false;
  }

  function onIterMenuOutside(e: MouseEvent): void {
    if (!iterMenuOpen) return;
    const el = e.target as HTMLElement;
    if (el.closest('.iterpill')) return;
    iterMenuOpen = false;
  }

  function dupCard() {
    duplicateInPane(paneKey, card.id);
  }
  // Round 2, item 1 — instant delete, no confirm modal, but a toast holds a
  // real undo for a few seconds. `card`/`index` are captured synchronously
  // (before the store update below), so the restore lands the exact same
  // object back at its exact prior position, not just re-appended.
  function delCard() {
    const snapshot = card;
    const at = index;
    removeFromPane(paneKey, card.id);
    showToast('Card deleted', { label: 'Undo', onClick: () => insertCardIntoPane(paneKey, at, snapshot) });
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

<svelte:window on:keydown={onOverflowKeydown} />
<svelte:body on:mousedown|capture={onIterMenuOutside} />

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
    <!-- Goal on its own full-width line — round 2, item 2: a `ChipInput`
         (contenteditable, atomic resolved-token chips), not a plain
         `<textarea>`, so a resolved `:alias`/`@repo`/`;model/opus`/`×N`
         renders inline in place rather than in a separate row. Still honors
         `:alias @repo ×N` on commit either way — nothing about the
         underlying `card.goal` string changed. -->
    <div class="goalwrap">
      <ChipInput
        bind:rootEl={goalInput}
        value={card.goal}
        segments={goalSegments}
        onInput={onGoalInput}
        onKeydown={onGoalKeydown}
        onFocus={() => (goalFocused = true)}
        onBlur={() => (goalFocused = false)}
        placeholder="describe the prompt or goal..."
      />
      {#if showAliasSuggest}
        <AutocompleteSuggest
          anchor={goalInput}
          items={aliasMatches.map((m) => ({ value: m.alias, label: m.label, hint: m.hint, kind: 'alias' }))}
          activeIndex={aliasActiveIndex}
          onSelect={selectAlias}
        />
      {:else if showRepoSuggest}
        <AutocompleteSuggest
          anchor={goalInput}
          items={repoMatches.map((m) => ({ value: m.token, label: m.label, hint: m.hint, kind: 'repo' }))}
          activeIndex={repoActiveIndex}
          onSelect={selectRepo}
        />
      {:else if showCmdSuggest}
        <AutocompleteSuggest
          anchor={goalInput}
          items={cmdMatches.map((m) => ({ value: m.token, label: m.label, hint: m.hint, kind: pendingCommand ?? (m as CommandSuggestion).command }))}
          activeIndex={cmdActiveIndex}
          onSelect={selectCommand}
        />
      {:else if showClaudeSuggest}
        <AutocompleteSuggest
          anchor={goalInput}
          items={claudeMatches.map((m) => ({ value: m.token, label: m.name, hint: m.hint, kind: 'claude' }))}
          activeIndex={claudeActiveIndex}
          onSelect={selectClaudeCommand}
        />
      {:else if showLoopSuggest}
        <AutocompleteSuggest
          anchor={goalInput}
          items={loopMatches.map((m) => ({ value: m.token, label: m.label, hint: m.hint, kind: 'loop' }))}
          activeIndex={loopActiveIndex}
          onSelect={selectLoop}
        />
      {/if}
    </div>
    <div class="grammarchips">
      <button type="button" class="gchip alias" on:click={chipAlias}>:alias</button>
      <button type="button" class="gchip repo" on:click={chipRepo}>@repo</button>
      <button type="button" class="gchip model" on:click={() => chipCommand('model')}>;model</button>
      <button type="button" class="gchip effort" on:click={() => chipCommand('effort')}>;effort</button>
      <button type="button" class="gchip loop" on:click={chipLoop}>×N</button>
      {#if claudeCommandOptions.length > 0}
        <button type="button" class="gchip claude" on:click={chipClaude}>/cmd</button>
      {/if}
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

  {#if showSep && !isRunning}
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
      <span class="iterbody">
        <button
          type="button"
          class="lb"
          disabled={loopRunning}
          aria-haspopup="listbox"
          aria-expanded={iterMenuOpen}
          title="pick iteration count"
          on:click={() => (iterMenuOpen = !iterMenuOpen)}
          >{@html loopRunning ? ICONS.spinner : ICONS.loop}<span class="val"
            >{loopRunning
              ? `${card.iteration?.current}/${card.iteration?.total}`
              : card.maxIterations === 0
                ? 'off'
                : '×' + cardIterationsLabel(card.maxIterations)}</span
          ></button
        >
        <span class="steppers">
          <button class="sb" on:click={() => step(-1)} title="fewer iterations">−</button>
          <button class="sb" on:click={() => step(1)} title="more iterations">+</button>
        </span>
      </span>
      {#if iterMenuOpen}
        <ul class="itermenu" role="listbox">
          <li role="option" aria-selected={card.maxIterations === 0}>
            <button type="button" class:sel={card.maxIterations === 0} on:click={() => pickIterations(0)}>off</button>
          </li>
          {#each ITER_PICK_VALUES as n (n)}
            <li role="option" aria-selected={card.maxIterations === n}>
              <button type="button" class:sel={card.maxIterations === n} on:click={() => pickIterations(n)}
                >×{n}</button
              >
            </li>
          {/each}
        </ul>
      {/if}
    </span>
    {#if showRunStats && liveAgent}
      <RunStatsPill
        elapsedMs={liveAgent.elapsedMs}
        tokens={(liveAgent.outputTokens ?? 0) + (liveAgent.inputTokens ?? 0)}
        costUsd={liveAgent.cost}
      />
    {/if}

    {#if isRunning}
      <!-- Claude-Desktop-style running view (UI-3): every facet/ops control
           below collapses behind this one button — only the loop pill (and
           the live run-stats pill above) stay inline while a card runs. -->
      <span class="sp"></span>
      <div class="overflowwrap">
        <button
          class="ib overflow"
          class:act={overflowOpen}
          bind:this={overflowBtn}
          type="button"
          aria-expanded={overflowOpen}
          aria-label="more controls"
          title="more controls"
          on:click={() => (overflowOpen = !overflowOpen)}
        >
          {@html ICONS.more}
        </button>
        {#if overflowOpen}
          <div class="overflowmenu" role="menu">
            {#if showSep}
              <div class="omsum">
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
              </div>
              <hr class="omsep" />
            {/if}
            <div class="omrow">
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
                class="ib goal"
                class:act={goalOn}
                type="button"
                bind:this={goalBtn}
                on:click={() => togglePopover(goalId)}
                aria-pressed={goalOn}
                title="pursue this loop's own acceptance goal"
              >
                {@html ICONS.gauge}
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
            </div>
            <div class="omrow">
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
            </div>
          </div>
        {/if}
      </div>
    {:else}
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
        class="ib goal"
        class:act={goalOn}
        type="button"
        bind:this={goalBtn}
        on:click={() => togglePopover(goalId)}
        aria-pressed={goalOn}
        title="pursue this loop's own acceptance goal"
      >
        {@html ICONS.gauge}
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
    budgetPreset={card.guardrails.budgetPreset}
    budgetUsd={card.guardrails.budgetUsd}
    isolation={card.guardrails.isolation}
    noProgressLimit={card.guardrails.noProgressLimit}
    onChangeGate={(patch) => writeCard({ guardrails: { ...card.guardrails, ...patch } })}
    onChangeUntil={(patch) => writeCard({ guardrails: { ...card.guardrails, ...patch } })}
    onChangeOnFail={(onFail) => writeCard({ guardrails: { ...card.guardrails, onFail } })}
    onChangeBudget={(budget) => writeCard({ guardrails: { ...card.guardrails, budget } })}
    onChangeBudgetPreset={(budgetPreset) => writeCard({ guardrails: { ...card.guardrails, budgetPreset } })}
    onChangeBudgetUsd={(budgetUsd) => writeCard({ guardrails: { ...card.guardrails, budgetUsd } })}
    onChangeIsolation={(isolation) => writeCard({ guardrails: { ...card.guardrails, isolation } })}
    onChangeNoProgressLimit={(noProgressLimit) => writeCard({ guardrails: { ...card.guardrails, noProgressLimit } })}
    maxIterations={card.maxIterations}
    onStep={step}
  />
</Popover>
<Popover id={evalId} anchor={evalBtn ?? null} kind="eval">
  <EvalsPopover evals={card.evals} onChange={(evals) => writeCard({ evals })} />
</Popover>
<Popover id={goalId} anchor={goalBtn ?? null} kind="goal">
  <GoalPopover
    scope="card"
    pursue={card.goalPursuit.pursue}
    pursues={goalPursues}
    onTogglePursue={() => writeCard({ goalPursuit: { ...card.goalPursuit, pursue: !card.goalPursuit.pursue } })}
  />
</Popover>

<style>
  .pc {
    position: relative;
    background: var(--konjo-card, var(--k-ext-surface-panel));
    border: 1px solid rgb(var(--k-wash-rgb) / 0.14);
    border-radius: 9px;
    padding: 13px 14px;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    box-shadow:
      inset 0 1px 0 rgb(var(--k-wash-rgb) / 0.08),
      0 1px 2px rgb(var(--k-shadow-rgb) / 0.4);
    transition:
      box-shadow 0.12s,
      border-color 0.12s;
  }
  /* UI-3 — a running card sheds its "card" chrome entirely: no background,
     no border, no shadow. The prompt + its cardbar sit straight on the
     pane's own background instead of inside a boxed widget (Claude
     Desktop's running-turn look); the runtag badge's own pulse is the sole
     "this is alive" signal now that the border-flash animation is gone. */
  .pc.running {
    background: transparent;
    border-color: transparent;
    box-shadow: none;
    animation: none;
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
    border-color: rgb(var(--k-danger-rgb) / 0.45);
  }
  /* Draft card (Creation-Flow-1): dashed until it carries content, then a
     teal "hot" border signalling it's ready to commit. */
  .pc.draft {
    border-style: dashed;
    border-color: rgb(var(--k-wash-rgb) / 0.18);
  }
  .pc.draft.hot {
    border-style: solid;
    border-color: rgb(var(--k-chip-alias-rgb) / 0.5);
    box-shadow: 0 0 18px rgb(var(--k-chip-alias-rgb) / 0.08);
  }
  .runtag.draft {
    color: rgb(var(--k-text-primary-rgb) / 0.46);
  }
  .pc.draft.hot .runtag.draft {
    color: var(--stack-teal, var(--k-chip-alias));
    border-color: rgb(var(--k-chip-alias-rgb) / 0.45);
  }
  .draftspec {
    row-gap: 7px;
  }
  .goalwrap {
    position: relative;
    margin-top: 10px;
  }
  /* `ChipInput`'s root is rendered by a child component, so it never carries
     this component's own scoping hash — `:global()` scoped through
     `.goalwrap` (which DOES belong to this template) is how a parent styles
     into a child's internal DOM in Svelte, and keeps this from leaking to
     every other `ChipInput` instance on the page (e.g. the stack dock's
     cmdbar, which wants its own orange-focus/smaller-font treatment). */
  :global(.goalwrap .chipinput) {
    background: rgb(var(--k-wash-rgb) / 0.02);
    border: 1px solid rgb(var(--k-wash-rgb) / 0.11);
    border-radius: 7px;
    padding: 9px 11px;
    color: var(--konjo-paper, var(--k-text-primary));
    font-size: 14px;
    transition:
      border-color 0.12s,
      background 0.12s;
  }
  :global(.goalwrap .chipinput:focus) {
    border-color: rgb(var(--k-chip-alias-rgb) / 0.4);
    background: rgb(var(--k-chip-alias-rgb) / 0.03);
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
    border: 1px solid rgb(var(--k-chip-alias-rgb) / 0.4);
    color: var(--stack-teal, var(--k-chip-alias));
  }
  .gchip.alias:hover {
    border-color: rgb(var(--k-chip-alias-rgb) / 0.7);
    background: rgb(var(--k-chip-alias-rgb) / 0.08);
  }
  .gchip.repo {
    border: 1px solid rgb(var(--k-chip-repo-rgb) / 0.4);
    color: var(--konjo-ice, var(--k-chip-repo));
  }
  .gchip.repo:hover {
    border-color: rgb(var(--k-chip-repo-rgb) / 0.7);
    background: rgb(var(--k-chip-repo-rgb) / 0.08);
  }
  /* violet — matches ChipInput.svelte's `chip-model` (the same color the
     resolved `;model/…` chip renders in once picked) and ConfigDrawer.svelte's
     model accent. */
  .gchip.model {
    border: 1px solid rgb(var(--k-chip-model-rgb) / 0.4);
    color: var(--stack-violet, var(--k-chip-model));
  }
  .gchip.model:hover {
    border-color: rgb(var(--k-chip-model-rgb) / 0.7);
    background: rgb(var(--k-chip-model-rgb) / 0.08);
  }
  /* sun, not ember — matches ChipInput.svelte's `chip-effort` and
     ConfigDrawer.svelte's effort accent. */
  .gchip.effort {
    border: 1px solid rgb(var(--k-chip-effort-rgb) / 0.4);
    color: var(--konjo-sun, var(--k-chip-effort));
  }
  .gchip.effort:hover {
    border-color: rgb(var(--k-chip-effort-rgb) / 0.7);
    background: rgb(var(--k-chip-effort-rgb) / 0.08);
  }
  /* flame, not sun — matches ChipInput.svelte's `chip-loop` and the card's
     own `.iterpill` (the "actual loop button"), so the ×N grammar chip and
     the loop control it feeds read as the same color. */
  .gchip.loop {
    border: 1px solid rgb(var(--k-chip-loop-rgb) / 0.4);
    color: var(--konjo-flame, var(--k-chip-loop));
  }
  .gchip.loop:hover {
    border-color: rgb(var(--k-chip-loop-rgb) / 0.7);
    background: rgb(var(--k-chip-loop-rgb) / 0.08);
  }
  .gchip.claude {
    border: 1px solid rgb(var(--k-danger-rgb) / 0.4);
    color: var(--konjo-rose, var(--k-danger));
  }
  .gchip.claude:hover {
    border-color: rgb(var(--k-danger-rgb) / 0.7);
    background: rgb(var(--k-danger-rgb) / 0.08);
  }
  .ib.add {
    color: var(--konjo-jade, var(--k-preset-benchmark));
    border-color: rgb(var(--k-preset-benchmark-rgb) / 0.5);
    background: rgb(var(--k-preset-benchmark-rgb) / 0.08);
    font-weight: 700;
    padding: 0 12px;
  }
  .ib.add .addlbl {
    font-size: 11px;
  }
  .ib.add:hover:not(:disabled) {
    color: var(--konjo-jade, var(--k-preset-benchmark));
    border-color: rgb(var(--k-preset-benchmark-rgb) / 0.8);
    background: rgb(var(--k-preset-benchmark-rgb) / 0.14);
  }
  .ib.add:disabled {
    opacity: 0.4;
    cursor: not-allowed;
    color: rgb(var(--k-text-primary-rgb) / 0.28);
    border-color: rgb(var(--k-wash-rgb) / 0.11);
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
    background: var(--konjo-black, var(--k-ext-black-fallback));
    border: 1px solid rgb(var(--k-wash-rgb) / 0.11);
    border-radius: 3px;
    padding: 2px 8px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: rgb(var(--k-text-primary-rgb) / 0.46);
    z-index: 2;
  }
  .runtag.running {
    color: var(--konjo-flame, var(--k-chip-loop));
    border-color: rgb(var(--k-chip-loop-rgb) / 0.5);
  }
  .runtag.running::before {
    content: '';
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--konjo-flame, var(--k-chip-loop));
    box-shadow: 0 0 5px var(--konjo-ember, var(--k-ext-ember));
    animation: pulse 1.4s infinite;
  }
  .runtag.queued {
    color: var(--konjo-ice, var(--k-chip-repo));
    border-color: rgb(var(--k-chip-repo-rgb) / 0.45);
  }
  .runtag.done {
    color: var(--konjo-jade, var(--k-preset-benchmark));
    border-color: rgb(var(--k-preset-benchmark-rgb) / 0.45);
  }
  .runtag.blocked {
    color: var(--konjo-rose, var(--k-danger));
    border-color: rgb(var(--k-danger-rgb) / 0.5);
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
    background: rgb(var(--k-danger-rgb) / 0.08);
    color: var(--k-ext-stackcard-pink);
    font-size: 10px;
    line-height: 1.4;
  }
  .blockreason :global(svg) {
    width: 12px;
    height: 12px;
    flex: 0 0 auto;
    color: var(--konjo-rose, var(--k-danger));
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
    color: rgb(var(--k-text-primary-rgb) / 0.46);
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
  .iterbar {
    display: flex;
    gap: 4px;
    margin-top: 9px;
  }
  .iterbar i {
    height: 3px;
    width: 22px;
    border-radius: 2px;
    background: rgb(var(--k-wash-rgb) / 0.11);
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
    background: rgb(var(--k-wash-rgb) / 0.05);
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
    border: 1px solid rgb(var(--k-wash-rgb) / 0.16);
    background: rgb(var(--k-wash-rgb) / 0.04);
    color: rgb(var(--k-text-primary-rgb) / 0.7);
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    font-size: 10px;
    cursor: pointer;
    transition: 0.12s;
  }
  .sumchip:hover {
    border-color: rgb(var(--k-wash-rgb) / 0.32);
    background: rgb(var(--k-wash-rgb) / 0.08);
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
    color: rgb(var(--k-text-primary-rgb) / 0.6);
  }
  .sumln.max .rl {
    color: rgb(var(--k-text-primary-rgb) / 0.6);
  }
  .sumln.guard .rl {
    color: rgb(var(--k-text-primary-rgb) / 0.6);
  }
  .sumln.eval .rl {
    color: rgb(var(--k-text-primary-rgb) / 0.6);
  }
  .sumln.cfg .rl {
    color: rgb(var(--k-text-primary-rgb) / 0.6);
  }
  .sumln .txt {
    color: rgb(var(--k-text-primary-rgb) / 0.46);
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
    color: rgb(var(--k-text-primary-rgb) / 0.28);
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
    border: 1px solid rgb(var(--k-wash-rgb) / 0.11);
    background: transparent;
    color: rgb(var(--k-text-primary-rgb) / 0.28);
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
    color: var(--konjo-paper, var(--k-text-primary));
    border-color: rgb(var(--k-text-primary-rgb) / 0.46);
  }
  .ib .cnt {
    font-size: 9px;
    font-weight: 700;
  }
  .ib.sched.act {
    color: var(--k-text-primary);
    border-color: rgb(var(--k-wash-rgb) / 0.5);
    background: rgb(var(--k-wash-rgb) / 0.1);
  }
  .ib.max.act {
    color: var(--k-text-primary);
    border-color: rgb(var(--k-wash-rgb) / 0.5);
    background: rgb(var(--k-wash-rgb) / 0.1);
  }
  .ib.danger:hover {
    color: var(--konjo-rose, var(--k-danger));
    border-color: rgb(var(--k-danger-rgb) / 0.4);
  }
  .ib.guard.act {
    color: var(--k-text-primary);
    border-color: rgb(var(--k-wash-rgb) / 0.5);
    background: rgb(var(--k-wash-rgb) / 0.1);
  }
  .ib.eval.act {
    color: var(--k-text-primary);
    border-color: rgb(var(--k-wash-rgb) / 0.5);
    background: rgb(var(--k-wash-rgb) / 0.1);
  }
  .ib.goal.act {
    color: var(--k-text-primary);
    border-color: rgb(var(--k-wash-rgb) / 0.5);
    background: rgb(var(--k-wash-rgb) / 0.1);
  }
  .ib.config.act {
    color: var(--k-text-primary);
    border-color: rgb(var(--k-wash-rgb) / 0.5);
    background: rgb(var(--k-wash-rgb) / 0.1);
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
  /* UI-3 running-view overflow — the "..." trigger + its floating menu.
     Not a `Popover.svelte` instance: that component unmounts its slot
     content whenever a *different* popover id becomes active, which would
     tear down `schedBtn`/`guardBtn`/… (rendered inside this menu) the
     instant one of their own facet popovers opened — this local toggle
     keeps them mounted for as long as the menu itself stays open. */
  .overflowwrap {
    position: relative;
    flex: 0 0 auto;
  }
  .ib.overflow.act {
    color: var(--k-text-primary);
    border-color: rgb(var(--k-text-primary-rgb) / 0.5);
    background: rgb(var(--k-text-primary-rgb) / 0.1);
  }
  .overflowmenu {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 20;
    width: max-content;
    min-width: 220px;
    max-width: 320px;
    padding: 10px;
    border-radius: 9px;
    background: var(--konjo-panel, var(--k-surface-raised));
    border: 1px solid rgb(var(--k-wash-rgb) / 0.14);
    box-shadow: 0 14px 40px rgb(var(--k-shadow-rgb) / 0.6);
  }
  .overflowmenu .omsum {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .overflowmenu .omsep {
    height: 1px;
    background: rgb(var(--k-wash-rgb) / 0.08);
    border: none;
    margin: 9px 0;
  }
  .overflowmenu .omrow {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }
  .overflowmenu .omrow + .omrow {
    margin-top: 8px;
  }
  .iterpill {
    display: inline-flex;
    align-items: center;
    height: 29px;
    border: 1px solid rgb(var(--k-chip-loop-rgb) / 0.5);
    background: rgb(var(--k-ext-ember-rgb) / 0.09);
    border-radius: 6px;
    /* Position context for `.itermenu` below, and no clip of its own — the
       pill's rounded-rect clip moved onto `.iterbody` (which is exactly
       `.iterpill`'s own size), so the dropdown it hosts can escape past the
       pill's edge instead of being cut off by it. */
    position: relative;
    font-size: 11px;
    color: var(--konjo-flame);
    font-weight: 700;
  }
  .iterbody {
    display: inline-flex;
    align-items: center;
    height: 100%;
    border-radius: inherit;
    overflow: hidden;
  }
  .iterpill .lb {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 0 9px;
    height: 100%;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    font-weight: 700;
    cursor: pointer;
  }
  .iterpill .lb:disabled {
    cursor: default;
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
    border-left: 1px solid rgb(var(--k-chip-loop-rgb) / 0.35);
    background: transparent;
    color: var(--konjo-flame);
    font-size: 15px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .iterpill .sb:hover {
    background: rgb(var(--k-chip-loop-rgb) / 0.2);
  }
  /* The ×N direct-pick dropdown — off + 1 through 10 in a small floating
     list under the pill, same flame accent as the pill itself. */
  .itermenu {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 40;
    margin: 0;
    padding: 4px;
    list-style: none;
    min-width: 88px;
    max-height: 220px;
    overflow-y: auto;
    background: var(--konjo-panel, var(--k-surface-raised));
    border: 1px solid rgb(var(--k-chip-loop-rgb) / 0.35);
    border-radius: 9px;
    box-shadow: 0 12px 40px rgb(var(--k-shadow-rgb) / 0.6);
  }
  .itermenu button {
    display: block;
    width: 100%;
    padding: 6px 10px;
    border: none;
    border-radius: 6px;
    background: transparent;
    color: rgb(var(--k-text-primary-rgb) / 0.7);
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    font-weight: 700;
    text-align: left;
    cursor: pointer;
  }
  .itermenu button:hover {
    background: rgb(var(--k-chip-loop-rgb) / 0.14);
    color: var(--konjo-flame);
  }
  .itermenu button.sel {
    color: var(--konjo-flame);
    background: rgb(var(--k-chip-loop-rgb) / 0.1);
  }
  .iterpill.off {
    border-color: rgb(var(--k-text-primary-rgb) / 0.22);
    background: rgb(var(--k-text-primary-rgb) / 0.05);
    color: rgb(var(--k-text-primary-rgb) / 0.4);
  }
  .iterpill.off .sb {
    border-left-color: rgb(var(--k-text-primary-rgb) / 0.16);
    color: rgb(var(--k-text-primary-rgb) / 0.4);
  }
  .iterpill.off .sb:hover {
    background: rgb(var(--k-text-primary-rgb) / 0.08);
  }
  /* ×N color ramp (round 2, item 5) — untagged pill stays the pre-ramp
     orange baseline; these two classes are the only overrides needed. */
  .iterpill.tier-yellow {
    border-color: rgb(var(--k-chip-effort-rgb) / 0.5);
    background: rgb(var(--k-chip-effort-rgb) / 0.08);
    color: var(--k-chip-effort);
  }
  .iterpill.tier-yellow .sb {
    border-left-color: rgb(var(--k-chip-effort-rgb) / 0.35);
    color: var(--k-chip-effort);
  }
  .iterpill.tier-yellow .sb:hover {
    background: rgb(var(--k-chip-effort-rgb) / 0.2);
  }
  .iterpill.tier-red {
    border-color: rgb(var(--k-danger-rgb) / 0.5);
    background: rgb(var(--k-danger-rgb) / 0.1);
    color: var(--k-danger);
  }
  .iterpill.tier-red .sb {
    border-left-color: rgb(var(--k-danger-rgb) / 0.35);
    color: var(--k-danger);
  }
  .iterpill.tier-red .sb:hover {
    background: rgb(var(--k-danger-rgb) / 0.2);
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
      box-shadow: 0 0 0 0 rgb(var(--k-chip-loop-rgb) / 0);
      border-color: rgb(var(--k-chip-loop-rgb) / 0.5);
    }
    50% {
      box-shadow: 0 0 14px 1px rgb(var(--k-chip-loop-rgb) / 0.45);
      border-color: rgb(var(--k-chip-loop-rgb) / 0.95);
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
