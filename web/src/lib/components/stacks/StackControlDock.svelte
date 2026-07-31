<!--
  StackControlDock — the purple stack control area at the base of each
  pane (Stack-1). Reuses the exact per-loop controls (`Popover.svelte`, the
  iteration-pill stepper, the schedule/guardrails/evals/config popovers),
  just scoped to the whole stack instead of one card, plus the pane's
  already-real run/pause/resume/drain machinery (moved here verbatim from
  `StackPane.svelte`'s old plain footer — the mockup shows exactly one run
  button, not two).

  Two placement modes, per `docs/ui/lopi-stack-control-area.html`:
  `STACK_CONTROL_MODE === 'dock'` (shipped default) is a collapsible strip —
  header (STACK chip + summary + chevron) always visible, controls expand
  in the middle, run stays pinned at the bottom in both states.
  `'sticky'` is the always-fully-expanded variant; its CSS ships below,
  unused while the constant reads `'dock'` (`stores/stack.ts`'s doc comment
  on the constant explains the migration path — same shape as
  `SIDEBAR_MODE`).
-->
<script lang="ts">
  import { tick } from 'svelte';
  import { get } from 'svelte/store';
  import {
    type StackPaneState,
    panes,
    STACK_CONTROL_MODE,
    stackGuardActive,
    stackEvalActive,
    stackDefaultsActive,
    stackGoalActive,
    stackPursuesGoal,
    stackGuardSummary,
    stackEvalsSummary,
    stackGoalSummary,
    maxIterationsLabel,
    stepMaxIterations,
    loopCountTier,
    cronHuman,
    updateStackConfig,
    duplicateStackInPanes,
    deleteStackFromPanes,
    insertPaneIntoPanes,
    reorderStacksInPanes,
    STACK_COMMANDS,
    commandAutocomplete,
    commandValueAutocomplete,
    detectPendingCommand,
    evalSuiteOptions,
    applySuite,
    EVAL_SUITES,
    aliasAutocomplete,
    resolvePresetAlias,
    PRESET_CATALOG,
    HIGH_N_CONFIRM_THRESHOLD,
    estimateRunCost,
    tokenizeGoalChips,
    claudeCommandAutocomplete,
    type DryRunResult,
    type CommandSuggestion,
    type CommandValueSuggestion
  } from '$lib/stores/stack';
  import {
    runs,
    runStack,
    pauseStack,
    resumeStack,
    syncStackSchedule,
    type RunPhase
  } from '$lib/stores/stackRun';
  import { stackStopLabel } from '$lib/stores/stackGoal';
  import { showToast } from '$lib/stores/toastStore';
  import { agents } from '$lib/stores/agents';
  import { labelFor, type Option } from '$lib/stores/controls';
  import { modelCatalog, modelOptionsFrom, ensureModelCatalog } from '$lib/stores/modelCatalog';
  import { MODEL_OPTIONS, EFFORT_OPTIONS } from '$lib/stores/options';
  import { AUTONOMY_OPTIONS } from '$lib/stores/stackDefaults';
  import { branchesByRepo, branchOptionsFor, ensureBranches } from '$lib/stores/branches';
  import { claudeCommandsByRepo, claudeCommandOptionsFor, ensureClaudeCommands } from '$lib/stores/claudeCommands';
  import { repoAutocomplete, repoLabelForPath } from '$lib/stores/repoMenu';
  import { draggingPane, armedPaneKey } from './dnd';
  import { ICONS } from './icons';
  import Popover, { togglePopover, activePopoverId } from './Popover.svelte';
  import SchedulePopover from './SchedulePopover.svelte';
  import GuardrailsPopover from './GuardrailsPopover.svelte';
  import EvalsPopover from './EvalsPopover.svelte';
  import StackConfigPopover from './StackConfigPopover.svelte';
  import GoalPopover from './GoalPopover.svelte';
  import StackTemplatesMenu from './StackTemplatesMenu.svelte';
  import RunMenu from './RunMenu.svelte';
  import AutocompleteSuggest from './AutocompleteSuggest.svelte';
  import ChipInput from './ChipInput.svelte';

  export let pane: StackPaneState;
  /** This pane's index in `$panes` — the drag-drop source/target identity,
   *  mirroring `StackCard.svelte`'s own `index` prop one level up. */
  export let index: number;
  export let repoOptions: Option[] = [];

  $: config = pane.config;

  // Open by default so the loop/schedule/guardrails/config controls (and
  // the cmdbar) are visible without an extra click — adding prompts must
  // NOT close it (a card commit doesn't touch `dockOpen` at all), only an
  // actual run starting does, below.
  let dockOpen = true;
  let schedBtn: HTMLButtonElement | undefined;
  let guardBtn: HTMLButtonElement | undefined;
  let evalBtn: HTMLButtonElement | undefined;
  let cfgBtn: HTMLButtonElement | undefined;
  let goalBtn: HTMLButtonElement | undefined;

  $: schedId = `${pane.key}:stack:sched`;
  $: guardId = `${pane.key}:stack:guard`;
  $: evalId = `${pane.key}:stack:eval`;
  $: cfgId = `${pane.key}:stack:config`;
  $: goalId = `${pane.key}:stack:goal`;

  $: scheduledOn = config.scheduled;
  $: guardsOn = stackGuardActive(config.guardrails);
  $: evalsOn = stackEvalActive(config);
  $: configOn = stackDefaultsActive(config.defaults);
  // B1 — the goal facet. `goalOn` is the raw toggle; `pursues` is whether it
  // will actually drive run-until-goal (toggle on *and* real acceptance).
  $: goalOn = stackGoalActive(config);
  $: pursues = stackPursuesGoal(config);
  $: showSummary = scheduledOn || guardsOn || evalsOn || configOn || goalOn;

  function toggleGoal() {
    updateStackConfig(pane.key, { goal: { ...config.goal, pursue: !config.goal.pursue } });
  }

  $: void ensureModelCatalog();
  $: modelLabel = labelFor(modelOptionsFrom($modelCatalog), config.defaults.model);
  // A chosen repo previously vanished from the dock's own summary the
  // instant it was set — visible in the config popover (`StackConfigPopover`
  // reads `defaults.repo` directly) but nowhere else, since this line was
  // hardcoded to "model X · every loop inherits" with no repo term at all.
  $: repoLabel = config.defaults.repo ? repoLabelForPath(config.defaults.repo, repoOptions) : '';
  $: loopLabel = config.loopCount <= 1 ? maxIterationsLabel(config.loopCount) : '×' + maxIterationsLabel(config.loopCount);
  // ×N loop-count color ramp (round 2, item 5) — `null` while off (`1`); the
  // infinite sentinel (`0`) has no ceiling at all, so it always reads `red`.
  $: loopTier = config.loopCount === 1 ? null : loopCountTier(config.loopCount === 0 ? Infinity : config.loopCount);
  $: dockSummary = `${scheduledOn ? cronHuman(config.cron) + ' · ' : ''}loop ${loopLabel} · ${modelLabel}`;

  // Round 2, item 4 — live running cost total: summed straight from the
  // `agents` store (already reactive per-agent `cost`, the same field the
  // Overview COST column reads) for every card in this pane that has ever
  // launched a task. Not gated on `status === 'running'` — a finished card's
  // final cost still counts toward what this stack actually spent.
  $: runningTotal = pane.cards.reduce((sum, c) => sum + (c.taskId ? ($agents.get(c.taskId)?.cost ?? 0) : 0), 0);

  function stepLoop(delta: number) {
    updateStackConfig(pane.key, { loopCount: stepMaxIterations(config.loopCount, delta) });
  }

  // ── stack command bar (`@repo` / `;command`) ────────────────────────────────
  // The stack-only analogue of a card's goal-field autocomplete (Stack-1 §4):
  // several settings (stack schedule/guardrails/run-until-goal) have no
  // card-level equivalent to piggyback on, so they need their own text-entry
  // surface. Same `@`/`;` grammar as `StackCard.svelte`'s composer, writing
  // to `pane.config` instead of a card's `config`. Value-picker
  // commands apply immediately and clear the bar (no goal text to preserve
  // here); popover-openers reuse the exact `togglePopover(id)` calls the
  // dock's own icon buttons make.
  let cmdText = '';
  let cmdBarFocused = false;
  // Round 2, item 2 — the bar is a `ChipInput` (contenteditable), not a
  // plain `<textarea>`; see `StackCard.svelte`'s identical `goalInput`
  // rename comment for why every existing `cmdBarInput?.focus()`/
  // `anchor={cmdBarInput}` call site below keeps working unchanged.
  let cmdBarInput: HTMLDivElement | undefined;
  let cmdActiveIndex = 0;
  let cmdDismissed = false;
  let pendingCommand: string | null = null;

  $: void ensureBranches(config.defaults.repo);
  // Composer-Grammar-2 — same stack-default-repo resolution drives the real
  // Claude Code `/name` command catalog.
  $: void ensureClaudeCommands(config.defaults.repo);
  $: claudeCommandOptions = claudeCommandOptionsFor($claudeCommandsByRepo, config.defaults.repo);
  $: cmdBarSegments = tokenizeGoalChips(
    cmdText,
    STACK_COMMANDS,
    claudeCommandOptions.map((o) => o.value)
  );
  $: aliasMatches = aliasAutocomplete(cmdText);
  $: repoMatches = repoAutocomplete(cmdText, repoOptions);

  function commandOptionsFor(command: string): Option[] {
    switch (command) {
      case 'model':
        return MODEL_OPTIONS;
      case 'effort':
        return EFFORT_OPTIONS;
      case 'autonomy':
        return AUTONOMY_OPTIONS;
      case 'branch':
        return branchOptionsFor($branchesByRepo, config.defaults.repo);
      case 'eval':
        return evalSuiteOptions();
      default:
        return [];
    }
  }

  $: showAliasBarSuggest = cmdBarFocused && !cmdDismissed && !pendingCommand && aliasMatches.length > 0;
  $: showRepoBarSuggest =
    cmdBarFocused && !cmdDismissed && !pendingCommand && !showAliasBarSuggest && repoMatches.length > 0;
  $: cmdMatches = pendingCommand
    ? commandValueAutocomplete(cmdText, pendingCommand, commandOptionsFor(pendingCommand))
    : commandAutocomplete(cmdText, STACK_COMMANDS);
  $: showCmdBarSuggest =
    cmdBarFocused && !cmdDismissed && !showAliasBarSuggest && !showRepoBarSuggest && cmdMatches.length > 0;
  // Composer-Grammar-2 — lowest priority in the chain; mutually exclusive
  // with the others by construction (distinct trigger characters), same as
  // every other pairing here.
  $: claudeMatches = claudeCommandAutocomplete(cmdText, claudeCommandOptions);
  $: showClaudeBarSuggest =
    cmdBarFocused &&
    !cmdDismissed &&
    !showAliasBarSuggest &&
    !showRepoBarSuggest &&
    !showCmdBarSuggest &&
    claudeMatches.length > 0;
  $: activeMatchCount = showAliasBarSuggest
    ? aliasMatches.length
    : showRepoBarSuggest
      ? repoMatches.length
      : showCmdBarSuggest
        ? cmdMatches.length
        : claudeMatches.length;
  $: if (cmdActiveIndex >= activeMatchCount) {
    cmdActiveIndex = Math.max(0, activeMatchCount - 1);
  }
  // Re-infer `pendingCommand` from the typed text on every change — see
  // StackCard.svelte's identical comment for why relying only on
  // `selectCommand`'s explicit assignment misses hand-typed `;model/`.
  $: {
    const inferred = detectPendingCommand(cmdText, STACK_COMMANDS);
    if (inferred) {
      pendingCommand = inferred;
    } else if (pendingCommand && !new RegExp(`(^|\\s);${pendingCommand}/`).test(cmdText)) {
      pendingCommand = null;
    }
  }

  // `×N` has no `applyCommandValue` case of its own — unlike `;model`/
  // `;effort`, inserting/typing the literal token here (`chipLoopBar` or by
  // hand) never actually reached `config.loopCount` (the real "chain
  // repeats" field the dock's own stepper pill edits), so the chip rendered
  // but the stack's loop count silently never changed — the reported bug.
  // Detect a complete, word-bounded `xN`/`×N` token (the same pattern
  // `tokenizeGoalChips` chips it with) and apply it the moment it resolves.
  // Left visible in `cmdText` afterward, same as every other resolved token
  // in this bar (`selectAliasFromBar`/`selectRepoFromBar`/
  // `selectCommandFromBar` all leave their token in place rather than
  // clearing it) — the guard against `config.loopCount` avoids re-writing
  // the store on every unrelated keystroke once it already matches.
  $: {
    const loopMatch = /(^|\s)[×xX](\d+)(?=\s|$)/.exec(cmdText);
    const n = loopMatch ? parseInt(loopMatch[2], 10) : null;
    if (n && n > 0 && n !== config.loopCount) {
      updateStackConfig(pane.key, { loopCount: n });
    }
  }

  function applyDefault(patch: Partial<typeof config.defaults>) {
    updateStackConfig(pane.key, { defaults: { ...config.defaults, ...patch } });
  }

  function applyCommandValue(command: string, value: string): void {
    switch (command) {
      case 'eval':
        updateStackConfig(pane.key, { evals: applySuite(config.evals, EVAL_SUITES[value] ?? []) });
        return;
      case 'model':
        applyDefault({ model: value });
        return;
      case 'effort':
        applyDefault({ effort: value });
        return;
      case 'branch':
        applyDefault({ branch: value });
        return;
      case 'autonomy':
        applyDefault({ autonomy: value });
        return;
    }
  }

  function fireCommandAction(command: string): void {
    if (command === 'guard') togglePopover(guardId);
    else if (command === 'schedule') togglePopover(schedId);
    else if (command === 'goal') togglePopover(goalId);
  }

  // A picked preset alias has no dedicated stack-level field to land on (no
  // `pane.config.alias`) — the closest existing stack-scope equivalent to a
  // card's `applyPreset` is its eval suite, so selecting a preset here
  // attaches that preset's evals to the stack's chain acceptance, same as it
  // would attach to a fresh card. Splices the resolved token back into
  // `cmdText`, same pattern as `selectRepoFromBar` below.
  function selectAliasFromBar(alias: string): void {
    const key = resolvePresetAlias(alias.slice(1));
    if (key) updateStackConfig(pane.key, { evals: PRESET_CATALOG[key].evals });
    cmdText = `${alias} `;
    cmdActiveIndex = 0;
    cmdDismissed = true;
  }

  // Splices the resolved token back into `cmdText` (plus a trailing space)
  // rather than clearing it, mirroring `StackCard.svelte`'s `selectRepo`/
  // `selectCommand` — otherwise the bar goes blank the instant a repo or
  // command value is picked, and the only place the choice is visible is the
  // popovers/summary line, not the text input itself. `cmdDismissed = true`
  // closes the now-stale suggestion list (it would otherwise keep matching
  // its own just-inserted token and stay open); typing further clears it via
  // `onCmdBarInput`.
  function selectRepoFromBar(token: string): void {
    const suggestion = repoMatches.find((s) => s.token === token);
    if (suggestion) applyDefault({ repo: suggestion.value });
    cmdText = `${token} `;
    cmdActiveIndex = 0;
    cmdDismissed = true;
  }

  /** Same follow-through as `StackCard.svelte`'s identical
   *  `revealConfigSurfaceFor` — surface the popover the picked value
   *  actually landed in (`StackConfigPopover` for model/effort/branch/
   *  autonomy, the evals popover for eval) instead of leaving the only
   *  feedback to a quietly-highlighted icon. */
  function revealConfigSurfaceFor(command: string): void {
    activePopoverId.set(command === 'eval' ? evalId : cfgId);
  }

  function selectCommandFromBar(token: string): void {
    if (pendingCommand) {
      const valueMatches = cmdMatches as CommandValueSuggestion[];
      const suggestion = valueMatches.find((s) => s.token === token);
      if (suggestion) {
        applyCommandValue(pendingCommand, suggestion.value);
        revealConfigSurfaceFor(pendingCommand);
      }
      cmdText = `;${pendingCommand}/${suggestion?.value ?? ''} `;
      pendingCommand = null;
      cmdDismissed = true;
    } else {
      const command = token.slice(1);
      const def = STACK_COMMANDS.find((c) => c.command === command);
      if (def?.isValuePicker) {
        cmdText = `;${command}/`;
        pendingCommand = command;
      } else {
        fireCommandAction(command);
        cmdText = '';
        cmdDismissed = true;
      }
    }
    cmdActiveIndex = 0;
  }

  /** No config write, unlike `selectRepoFromBar`/`applyCommandValue` — a real
   *  Claude command carries no lopi-side facet, see `StackCard.svelte`'s
   *  identical `selectClaudeCommand` doc comment. */
  function selectClaudeCommandFromBar(token: string): void {
    cmdText = `${token} `;
    cmdActiveIndex = 0;
    cmdDismissed = true;
  }

  // ── grammar chips (always-visible entry points into the bar's own
  //    autocomplete above) ─────────────────────────────────────────────────
  function chipSpacer(text: string): string {
    return text.length > 0 && !/\s$/.test(text) ? ' ' : '';
  }

  function chipAliasBar(): void {
    cmdBarFocused = true;
    cmdDismissed = false;
    cmdText = `${cmdText}${chipSpacer(cmdText)}:`;
    void tick().then(() => cmdBarInput?.focus());
  }

  function chipRepoBar(): void {
    cmdBarFocused = true;
    cmdDismissed = false;
    cmdText = `${cmdText}${chipSpacer(cmdText)}@`;
    void tick().then(() => cmdBarInput?.focus());
  }

  function chipCommandBar(command: string): void {
    cmdBarFocused = true;
    cmdDismissed = false;
    selectCommandFromBar(`;${command}`);
    void tick().then(() => cmdBarInput?.focus());
  }

  /** `×N` has no `;loop/N` command grammar (killed — `xN` is the sole
   *  loop-count grammar) — inserts the literal token directly, mirroring
   *  `StackCard.svelte`'s `chipLoop`. */
  function chipLoopBar(): void {
    cmdBarFocused = true;
    cmdDismissed = false;
    cmdText = `${cmdText}${chipSpacer(cmdText)}x3 `;
    void tick().then(() => cmdBarInput?.focus());
  }

  /** No single command to auto-select (the repo's catalog is dynamic) —
   *  only opens the level-1 list, mirroring `chipAliasBar`/`chipRepoBar`'s
   *  bare-trigger shape, not `chipCommandBar`'s immediate level-2 jump. */
  function chipClaudeBar(): void {
    cmdBarFocused = true;
    cmdDismissed = false;
    cmdText = `${cmdText}${chipSpacer(cmdText)}/`;
    void tick().then(() => cmdBarInput?.focus());
  }

  /** `ChipInput`'s `onInput` hands back the plain serialized string directly
   *  — see `StackCard.svelte`'s identical `onGoalInput` doc comment. */
  function onCmdBarInput(value: string): void {
    cmdText = value;
    cmdDismissed = false;
  }

  function onCmdBarKeydown(e: KeyboardEvent): void {
    const showing = showAliasBarSuggest || showRepoBarSuggest || showCmdBarSuggest || showClaudeBarSuggest;
    const matches: Array<{ token: string }> = showAliasBarSuggest
      ? aliasMatches.map((m) => ({ token: m.alias }))
      : showRepoBarSuggest
        ? repoMatches
        : showCmdBarSuggest
          ? cmdMatches
          : claudeMatches;
    if (showing) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        cmdActiveIndex = (cmdActiveIndex + 1) % matches.length;
        return;
      }
      if (e.key === 'ArrowUp') {
        e.preventDefault();
        cmdActiveIndex = (cmdActiveIndex - 1 + matches.length) % matches.length;
        return;
      }
      if (e.key === 'Tab' || e.key === 'Enter') {
        e.preventDefault();
        if (showAliasBarSuggest) selectAliasFromBar(matches[cmdActiveIndex].token);
        else if (showRepoBarSuggest) selectRepoFromBar(matches[cmdActiveIndex].token);
        else if (showCmdBarSuggest) selectCommandFromBar(matches[cmdActiveIndex].token);
        else selectClaudeCommandFromBar(matches[cmdActiveIndex].token);
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        cmdDismissed = true;
        return;
      }
    }
    if (e.key === 'Escape') {
      cmdText = '';
      pendingCommand = null;
      return;
    }
    // The bar is a single-token command line, not free-form prose — a bare
    // Enter with no suggestion showing is a no-op (as it always was for the
    // plain `<input>` this replaced), not a newline insertion.
    if (e.key === 'Enter') {
      e.preventDefault();
    }
  }

  // ── run stack (moved verbatim from StackPane.svelte's old footer) ───────────
  let runMenuOpen = false;
  let dryRunResult: DryRunResult | null = null;

  $: runState = $runs.get(pane.key);
  $: phase = runState?.phase as RunPhase | undefined;
  // The stack's own loop reads as "actively running" once it's mid-run
  // (`phase === 'running'`) with a real chain repeat configured (`loopCount
  // !== 1` — 1 is the dock's own "off" sentinel, matching `IterationPill`'s
  // `offAtZero: false` convention). `repetition` is 0-indexed and counts
  // *completed* passes, so the live iteration is `repetition + 1`.
  $: stackLoopRunning = phase === 'running' && config.loopCount !== 1;
  $: stackIterCurrent = (runState?.repetition ?? 0) + 1;
  $: stackIterTotal = runState?.loopTarget ?? config.loopCount;
  $: stackIterLabel = stackIterTotal === 0 ? `${stackIterCurrent}/∞` : `${stackIterCurrent}/${stackIterTotal}`;
  // Auto-close only on the transition into an actual run — depends solely
  // on `phase`, so it fires once when the stack starts running and never
  // re-fires just because the user reopens the dock while still running.
  $: if (phase === 'running') dockOpen = false;
  $: runError = $runs.get(pane.key)?.error;
  // B1 — the specific reason a goal run halted (goal_met vs no_progress vs
  // max_chain_loops). When present it drives its own banner and supersedes the
  // generic error banner, so the outcome reads as a specific verdict.
  $: stopReason = $runs.get(pane.key)?.stopReason;
  $: runLabel =
    phase === 'running'
      ? 'pause'
      : phase === 'paused'
        ? 'resume'
        : phase === 'draining'
          ? 'draining…'
          : pursues
            ? 'pursue goal'
            : 'run stack';
  $: runIcon = phase === 'running' ? ICONS.pause : ICONS.play;

  // ── cost-estimate confirm above a high loop count (round 2, item 6) ─────────
  // A non-blocking inline row, not a modal: hitting Run with a high ×N holds
  // the launch for one extra click ("run anyway" / "lower to ×N") instead of
  // dispatching immediately. `0` (the infinite sentinel) always qualifies —
  // it has no ceiling, which is exactly the case most worth a pause.
  let costConfirmOpen = false;
  $: costEst = config.loopCount > 0 ? estimateRunCost(config.defaults.model, config.loopCount) : null;

  function launchRun() {
    dryRunResult = null;
    runStack(pane.key, 'run', config.defaults, agents);
  }

  function confirmRunAnyway() {
    costConfirmOpen = false;
    launchRun();
  }

  function confirmLowerAndRun() {
    costConfirmOpen = false;
    updateStackConfig(pane.key, { loopCount: 10 });
    launchRun();
  }

  function runMain() {
    if (phase === 'running') {
      pauseStack(pane.key);
    } else if (phase === 'paused') {
      resumeStack(pane.key, config.defaults, agents);
    } else if (config.loopCount === 0 || config.loopCount >= HIGH_N_CONFIRM_THRESHOLD) {
      costConfirmOpen = true;
    } else {
      launchRun();
    }
    runMenuOpen = false;
  }

  function dismissRunError() {
    runs.update((m) => {
      const next = new Map(m);
      next.delete(pane.key);
      return next;
    });
  }

  // ── stack ops: duplicate / drag-reorder / delete (Stack-1 Phase 1) ──────────
  function dupStack() {
    duplicateStackInPanes(pane.key);
  }
  // Round 2, item 1 — instant delete, no confirm modal, but a toast holds a
  // real undo for a few seconds, restoring the whole pane (cards + config)
  // at its exact prior position. `pane`/`index` are captured synchronously
  // before the store updates below.
  function delStack() {
    // `deleteStack` (stack.ts) refuses to remove the last remaining pane —
    // mirror that guard here so the toast never claims a delete that didn't
    // actually happen.
    if (get(panes).length <= 1) return;
    const snapshot = pane;
    const at = index;
    runs.update((m) => {
      const next = new Map(m);
      next.delete(pane.key);
      return next;
    });
    deleteStackFromPanes(pane.key);
    showToast('Stack deleted', { label: 'Undo', onClick: () => insertPaneIntoPanes(at, snapshot) });
  }
  // ── drop target: the dock's own root, before/after by cursor Y — mirrors
  //    StackCard.svelte's within-pane card drag exactly, one level up. ──────
  let dropBefore = false;
  let dropAfter = false;

  function onDockDragOver(e: DragEvent) {
    const cur = $draggingPane;
    if (!cur || cur.paneKey === pane.key) return;
    e.preventDefault();
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const before = e.clientY - rect.top < rect.height / 2;
    dropBefore = before;
    dropAfter = !before;
  }
  function onDockDragLeave() {
    dropBefore = false;
    dropAfter = false;
  }
  function onDockDrop(e: DragEvent) {
    e.preventDefault();
    const cur = $draggingPane;
    const before = dropBefore;
    dropBefore = false;
    dropAfter = false;
    if (!cur || cur.paneKey === pane.key) return;
    reorderStacksInPanes(cur.index, index, before);
  }
</script>

<div
  class="sctrl"
  class:dock={STACK_CONTROL_MODE === 'dock'}
  class:sticky={STACK_CONTROL_MODE === 'sticky'}
  class:open={dockOpen}
  class:drop-before={dropBefore}
  class:drop-after={dropAfter}
  role="group"
  aria-label="stack controls"
  on:dragover={onDockDragOver}
  on:dragleave={onDockDragLeave}
  on:drop={onDockDrop}
>
  {#if STACK_CONTROL_MODE === 'dock'}
    <div class="dockhead">
      <span class="stag">stack</span>
      {#if runningTotal > 0}
        <span class="costtotal"><span class="costlbl">running total:</span> ${runningTotal.toFixed(2)}</span>
      {/if}
      <span class="dsum">{dockSummary}</span>
      <!-- UI-3 — the stack/model/loop summary, the ^ open toggle and the
           run/pause action now all live in this single header row: the dock
           minimizes to one thin strip instead of a header row plus a
           separate full-width run button underneath. -->
      <div class="hrun">
        <button
          class="hrunbtn"
          type="button"
          on:click={runMain}
          disabled={phase === 'draining'}
          title="run this stack"
        >
          {@html runIcon}<span class="hrunlbl">{runLabel}</span>
        </button>
        <button
          class="runchev hrunchev"
          type="button"
          on:click={() => (runMenuOpen = !runMenuOpen)}
          aria-expanded={runMenuOpen}
          title="run options"
        >
          {@html ICONS.chevdown}
        </button>
        {#if runMenuOpen}
          <RunMenu
            paneKey={pane.key}
            defaults={config.defaults}
            {phase}
            onDryRun={(r) => (dryRunResult = r)}
            onClose={() => (runMenuOpen = false)}
            onRunNow={runMain}
          />
        {/if}
      </div>
      <button class="exp" type="button" on:click={() => (dockOpen = !dockOpen)} aria-expanded={dockOpen} title="stack controls">
        {@html ICONS.chevup}
      </button>
    </div>
  {:else}
    <div class="sctop">
      <span class="stag">stack</span>
      <span class="sp"></span>
    </div>
  {/if}

  {#if costConfirmOpen || stopReason || runError || dryRunResult}
    <div class="dockbanners">
      {#if costConfirmOpen}
        <div class="costconfirm">
          <span class="ccmsg">
            {@html ICONS.zap}
            {#if config.loopCount === 0}
              <b>×∞</b> on <b>{modelLabel}</b> — unbounded, no cost ceiling to estimate
            {:else if costEst}
              <b>×{config.loopCount}</b> on <b>{modelLabel}</b> ≈ <b>${costEst.low.toFixed(2)}–${costEst.high.toFixed(2)}</b>
              estimated (approximate)
            {/if}
          </span>
          <div class="ccactions">
            <button type="button" on:click={confirmRunAnyway}>run anyway</button>
            <button type="button" on:click={confirmLowerAndRun}>lower to ×10</button>
          </div>
        </div>
      {:else if stopReason}
        <div class="runbanner" class:err={stopReason !== 'goal_met'} class:ok={stopReason === 'goal_met'}>
          <span>{stackStopLabel(stopReason)}</span>
          <button type="button" on:click={dismissRunError}>{@html ICONS.x}</button>
        </div>
      {:else if runError}
        <div class="runbanner err">
          <span>{runError}</span>
          <button type="button" on:click={dismissRunError}>{@html ICONS.x}</button>
        </div>
      {:else if dryRunResult}
        <div class="runbanner" class:err={!dryRunResult.valid}>
          <span>
            {#if dryRunResult.valid}
              dry run: {dryRunResult.plan.length} loop{dryRunResult.plan.length === 1 ? '' : 's'} would run, in order
            {:else}
              dry run found {dryRunResult.issues.length} issue{dryRunResult.issues.length === 1 ? '' : 's'}: {dryRunResult
                .issues[0].message}
            {/if}
          </span>
          <button type="button" on:click={() => (dryRunResult = null)}>{@html ICONS.x}</button>
        </div>
      {/if}
    </div>
  {/if}

  <div class="dockbody">
    <div class="inner">
      <div class="cmdbarwrap">
        <ChipInput
          bind:rootEl={cmdBarInput}
          value={cmdText}
          segments={cmdBarSegments}
          onInput={onCmdBarInput}
          onKeydown={onCmdBarKeydown}
          onFocus={() => (cmdBarFocused = true)}
          onBlur={() => (cmdBarFocused = false)}
          placeholder="stack command..."
        />
        {#if showAliasBarSuggest}
          <AutocompleteSuggest
            anchor={cmdBarInput}
            items={aliasMatches.map((m) => ({ value: m.alias, label: m.label, hint: m.hint, kind: 'alias' }))}
            activeIndex={cmdActiveIndex}
            onSelect={selectAliasFromBar}
          />
        {:else if showRepoBarSuggest}
          <AutocompleteSuggest
            anchor={cmdBarInput}
            items={repoMatches.map((m) => ({ value: m.token, label: m.label, hint: m.hint, kind: 'repo' }))}
            activeIndex={cmdActiveIndex}
            onSelect={selectRepoFromBar}
          />
        {:else if showCmdBarSuggest}
          <AutocompleteSuggest
            anchor={cmdBarInput}
            items={cmdMatches.map((m) => ({ value: m.token, label: m.label, hint: m.hint, kind: pendingCommand ?? (m as CommandSuggestion).command }))}
            activeIndex={cmdActiveIndex}
            onSelect={selectCommandFromBar}
          />
        {:else if showClaudeBarSuggest}
          <AutocompleteSuggest
            anchor={cmdBarInput}
            items={claudeMatches.map((m) => ({ value: m.token, label: m.name, hint: m.hint, kind: 'claude' }))}
            activeIndex={cmdActiveIndex}
            onSelect={selectClaudeCommandFromBar}
          />
        {/if}
      </div>
      <div class="grammarchips">
        <button type="button" class="gchip alias" on:click={chipAliasBar}>:alias</button>
        <button type="button" class="gchip repo" on:click={chipRepoBar}>@repo</button>
        <button type="button" class="gchip model" on:click={() => chipCommandBar('model')}>;model</button>
        <button type="button" class="gchip effort" on:click={() => chipCommandBar('effort')}>;effort</button>
        <button type="button" class="gchip loop" on:click={chipLoopBar}>×N</button>
        {#if claudeCommandOptions.length > 0}
          <button type="button" class="gchip claude" on:click={chipClaudeBar}>/cmd</button>
        {/if}
      </div>
      {#if showSummary}
        {#if scheduledOn}
          <div class="sumln sched">
            <span class="rl">{@html ICONS.cron}schedule</span>
            <span class="txt"><b>{cronHuman(config.cron)}</b></span>
          </div>
        {/if}
        {#if guardsOn}
          <div class="sumln guard">
            <span class="rl">{@html ICONS.shield}guards</span>
            <span class="txt">{stackGuardSummary(config.guardrails)}</span>
          </div>
        {/if}
        {#if evalsOn}
          <div class="sumln eval">
            <span class="rl">{@html ICONS.checkbox}evals</span>
            <span class="txt">{stackEvalsSummary(config)}</span>
          </div>
        {/if}
        {#if goalOn}
          <div class="sumln goal">
            <span class="rl">{@html ICONS.gauge}goal</span>
            <span class="txt">{stackGoalSummary(config)}</span>
          </div>
          {#if !pursues}
            <div class="hintrow">add chain-acceptance evals for the goal to pursue — a goal with nothing to check is inert</div>
          {/if}
        {/if}
        {#if configOn}
          <div class="sumln cfg">
            <span class="rl">{@html ICONS.sliders}default</span>
            <span class="txt">model <b>{modelLabel}</b>{#if repoLabel}{' · repo '}<b>{repoLabel}</b>{/if} · every loop inherits</span>
          </div>
        {/if}
      {/if}

      <div class="cardbar">
        <span
          class="iterpill"
          class:off={config.loopCount === 1}
          class:running={stackLoopRunning}
          class:tier-yellow={loopTier === 'yellow'}
          class:tier-red={loopTier === 'red'}
          title={stackLoopRunning
            ? `chain-run ${stackIterLabel}`
            : config.loopCount === 1
              ? 'off · runs once, no repeat'
              : config.loopCount === 0
                ? 'unlimited · runs until guardrails or goal stop it'
                : undefined}
        >
          <span class="lb"
            >{@html stackLoopRunning ? ICONS.spinner : ICONS.loop}<span class="val"
              >{stackLoopRunning ? stackIterLabel : loopLabel}</span
            ></span
          >
          <span class="steppers">
            <button class="sb" type="button" on:click={() => stepLoop(-1)} title="fewer chain repeats">−</button>
            <button class="sb" type="button" on:click={() => stepLoop(1)} title="more chain repeats">+</button>
          </span>
        </span>
        <button class="ib sched" class:act={scheduledOn} bind:this={schedBtn} on:click={() => togglePopover(schedId)} title="schedule the stack">
          {@html ICONS.cron}
        </button>
        <button class="ib guard" class:act={guardsOn} bind:this={guardBtn} on:click={() => togglePopover(guardId)} title="stack guardrails">
          {@html ICONS.shield}
        </button>
        <button class="ib eval" class:act={evalsOn} bind:this={evalBtn} on:click={() => togglePopover(evalId)} title="stack evals">
          {@html ICONS.checkbox}<span class="cnt">{config.evals.length}</span>
        </button>
        <button
          class="ib goal"
          class:act={goalOn}
          type="button"
          bind:this={goalBtn}
          on:click={() => togglePopover(goalId)}
          aria-pressed={goalOn}
          title="run until the stack acceptance passes (goal-directed)"
        >
          {@html ICONS.gauge}
        </button>
        <button class="ib config" class:act={configOn} bind:this={cfgBtn} on:click={() => togglePopover(cfgId)} title="stack default config">
          {@html ICONS.sliders}
        </button>
        <span class="sp"></span>
        <StackTemplatesMenu paneKey={pane.key} cards={pane.cards} />
        <button class="ib" type="button" on:click={dupStack} title="duplicate stack">{@html ICONS.dup}</button>
        <button
          class="ib drag"
          type="button"
          title="drag to reorder stacks"
          on:mousedown={() => armedPaneKey.set(pane.key)}
          on:mouseup={() => armedPaneKey.set(null)}
        >
          {@html ICONS.drag}
        </button>
        <button class="ib danger" type="button" on:click={delStack} title="delete stack">{@html ICONS.trash}</button>
      </div>
    </div>
  </div>
</div>

<Popover id={schedId} anchor={schedBtn ?? null} kind="sched">
  <SchedulePopover
    scheduled={config.scheduled}
    cron={config.cron}
    onToggle={() => {
      updateStackConfig(pane.key, { scheduled: !config.scheduled });
      void syncStackSchedule(pane.key, config.defaults);
    }}
    onChange={(next) => {
      updateStackConfig(pane.key, { cron: next });
      void syncStackSchedule(pane.key, config.defaults);
    }}
  />
</Popover>
<Popover id={guardId} anchor={guardBtn ?? null} kind="guard">
  <GuardrailsPopover
    scope="stack"
    onFail={config.guardrails.onFail}
    onChangeOnFail={(onFail) => updateStackConfig(pane.key, { guardrails: { ...config.guardrails, onFail } })}
    maxIterations={config.loopCount}
    onStep={stepLoop}
    iterLabel="loop stacks"
  />
</Popover>
<Popover id={evalId} anchor={evalBtn ?? null} kind="eval">
  <EvalsPopover evals={config.evals} onChange={(evals) => updateStackConfig(pane.key, { evals })} heading="chain acceptance" />
</Popover>
<Popover id={cfgId} anchor={cfgBtn ?? null} kind="config">
  <StackConfigPopover
    defaults={config.defaults}
    onChange={(patch) => updateStackConfig(pane.key, { defaults: { ...config.defaults, ...patch } })}
    {repoOptions}
  />
</Popover>
<Popover id={goalId} anchor={goalBtn ?? null} kind="goal">
  <GoalPopover
    pursue={config.goal.pursue}
    noProgressLimit={config.goal.noProgressLimit}
    {pursues}
    onTogglePursue={toggleGoal}
    onChangeNoProgressLimit={(noProgressLimit) => updateStackConfig(pane.key, { goal: { ...config.goal, noProgressLimit } })}
  />
</Popover>

<style>
  .sctrl {
    position: relative;
    flex: 0 0 auto;
    background: linear-gradient(180deg, rgb(var(--k-ext-violet-dock-a-rgb) / 0.22), rgb(var(--k-ext-violet-dock-b-rgb) / 0.14));
    border: 1px solid rgb(var(--k-border-interactive-rgb) / 0.4);
    border-top: 1.5px solid rgb(var(--k-border-interactive-rgb) / 0.55);
    box-shadow:
      inset 0 1px 0 rgb(var(--k-wash-rgb) / 0.1),
      0 -10px 30px rgb(var(--k-ext-violet-dock-c-rgb) / 0.14);
    padding: 14px 16px 16px;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
  }
  .sctrl.dock {
    padding-top: 0;
  }
  /* Sticky mode's CSS ships unused while STACK_CONTROL_MODE === 'dock' — see
     the constant's doc comment in stores/stack.ts. */
  .sctrl.sticky {
    border: 1.5px solid rgb(var(--k-border-interactive-rgb) / 0.5);
    border-radius: 11px;
    margin-top: 14px;
    box-shadow: 0 6px 26px rgb(var(--k-ext-violet-dock-c-rgb) / 0.18);
  }
  .sctrl.sticky .dockbody {
    max-height: none;
    overflow: visible;
  }
  .stag {
    font-size: 9px;
    letter-spacing: 0.18em;
    text-transform: uppercase;
    color: var(--k-ext-dock-plum);
    background: var(--stack-violet, var(--k-chip-model));
    border-radius: 4px;
    padding: 3px 10px;
    font-weight: 700;
    flex: 0 0 auto;
  }
  .sctop {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 2px;
  }
  .sctop .sp {
    flex: 1;
  }
  .dockhead {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 13px 0 2px;
  }
  .dockhead .costtotal {
    font-size: 11px;
    color: var(--konjo-paper, var(--k-text-primary));
    font-weight: 700;
    white-space: nowrap;
    flex: 0 0 auto;
  }
  .dockhead .costtotal .costlbl {
    color: rgb(var(--k-text-primary-rgb) / 0.6);
    font-weight: 400;
    margin-right: 3px;
  }
  .dockhead .dsum {
    font-size: 10px;
    color: rgb(var(--k-text-primary-rgb) / 0.66);
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sctrl.dock.open .dockhead .dsum {
    display: none;
  }
  .dockhead .exp {
    width: 34px;
    height: 34px;
    border: 1px solid rgb(var(--k-wash-rgb) / 0.2);
    border-radius: 7px;
    background: rgb(var(--k-shadow-rgb) / 0.2);
    color: var(--stack-violet, var(--k-chip-model));
    display: flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    flex: 0 0 auto;
    margin-left: auto;
  }
  .dockhead .exp :global(svg) {
    width: 16px;
    height: 16px;
    transition: transform 0.2s;
  }
  .sctrl.dock.open .dockhead .exp :global(svg) {
    transform: rotate(180deg);
  }
  .dockbody {
    max-height: 0;
    overflow: hidden;
    transition: max-height 0.26s ease;
  }
  .sctrl.dock.open .dockbody {
    max-height: 420px;
  }
  .dockbody .inner {
    padding-top: 2px;
  }
  .cmdbarwrap {
    position: relative;
    /* Recolors the shared `:focus-visible` ring (app.css) from its default
       ice/cyan to the run-stack button's orange, so the keyboard-focus halo
       matches the border-color below instead of clashing with it. */
    --konjo-accent-rgb: 255 149 0;
  }
  /* `ChipInput`'s root is rendered by a child component — `:global()` scoped
     through `.cmdbarwrap` (this component's own template) is how a parent
     reaches into a child's internal DOM in Svelte; see the identical note in
     `StackCard.svelte`'s `.goalwrap .chipinput` rule. */
  :global(.cmdbarwrap .chipinput) {
    background: rgb(var(--k-wash-rgb) / 0.02);
    border: 1px solid rgb(var(--k-wash-rgb) / 0.11);
    border-radius: 7px;
    padding: 8px 10px;
    color: var(--konjo-paper, var(--k-text-primary));
    font-size: 12px;
    transition:
      border-color 0.12s,
      background 0.12s;
  }
  :global(.cmdbarwrap .chipinput:focus) {
    border-color: var(--konjo-flame, var(--k-chip-loop));
    background: rgb(var(--k-chip-loop-rgb) / 0.06);
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
  /* violet — see StackCard.svelte's identical `.gchip.model` comment. */
  .gchip.model {
    border: 1px solid rgb(var(--k-chip-model-rgb) / 0.4);
    color: var(--stack-violet, var(--k-chip-model));
  }
  .gchip.model:hover {
    border-color: rgb(var(--k-chip-model-rgb) / 0.7);
    background: rgb(var(--k-chip-model-rgb) / 0.08);
  }
  /* sun — see StackCard.svelte's identical `.gchip.effort` comment. */
  .gchip.effort {
    border: 1px solid rgb(var(--k-chip-effort-rgb) / 0.4);
    color: var(--konjo-sun, var(--k-chip-effort));
  }
  .gchip.effort:hover {
    border-color: rgb(var(--k-chip-effort-rgb) / 0.7);
    background: rgb(var(--k-chip-effort-rgb) / 0.08);
  }
  /* flame — see StackCard.svelte's identical `.gchip.loop` comment. */
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
  .hintrow {
    font-size: 9px;
    color: rgb(var(--k-text-primary-rgb) / 0.28);
    padding: 4px 0 0 71px;
  }
  .sumln {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-top: 8px;
    font-size: 9.5px;
  }
  .sumln:first-of-type {
    margin-top: 10px;
  }
  .sumln .rl {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    text-transform: uppercase;
    font-size: 8px;
    letter-spacing: 0.06em;
    width: 64px;
    flex: 0 0 auto;
  }
  .sumln .rl :global(svg) {
    width: 11px;
    height: 11px;
  }
  .sumln.sched .rl {
    color: rgb(var(--k-text-primary-rgb) / 0.6);
  }
  .sumln.guard .rl {
    color: rgb(var(--k-text-primary-rgb) / 0.6);
  }
  .sumln.eval .rl {
    color: rgb(var(--k-text-primary-rgb) / 0.6);
  }
  .sumln.goal .rl {
    color: rgb(var(--k-text-primary-rgb) / 0.6);
  }
  .sumln.cfg .rl {
    color: rgb(var(--k-text-primary-rgb) / 0.6);
  }
  .sumln .txt {
    color: rgb(var(--k-text-primary-rgb) / 0.66);
  }
  .sumln .txt b {
    color: var(--konjo-paper, var(--k-text-primary));
  }
  .sumln.sched .txt b {
    color: var(--konjo-ice);
  }
  .sumln.guard .txt b {
    color: var(--konjo-sun);
  }
  .cardbar {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 13px;
  }
  .ib {
    position: relative;
    height: 29px;
    min-width: 29px;
    padding: 0 7px;
    border-radius: 6px;
    border: 1px solid rgb(var(--k-wash-rgb) / 0.16);
    background: rgb(var(--k-shadow-rgb) / 0.18);
    color: rgb(var(--k-text-primary-rgb) / 0.66);
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
    border-color: rgb(var(--k-wash-rgb) / 0.32);
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
  .ib.config.act {
    color: var(--k-text-primary);
    border-color: rgb(var(--k-wash-rgb) / 0.5);
    background: rgb(var(--k-wash-rgb) / 0.1);
  }
  .ib.goal.act {
    color: var(--k-text-primary);
    border-color: rgb(var(--k-wash-rgb) / 0.5);
    background: rgb(var(--k-wash-rgb) / 0.1);
  }
  .ib.danger:hover {
    color: var(--konjo-rose, var(--k-danger));
    border-color: rgb(var(--k-danger-rgb) / 0.4);
  }
  .ib.drag {
    cursor: grab;
  }
  .sp {
    flex: 1;
  }
  .iterpill {
    display: inline-flex;
    align-items: center;
    height: 29px;
    border: 1px solid rgb(var(--k-chip-loop-rgb) / 0.6);
    background: rgb(var(--k-ext-orange-warn-rgb) / 0.16);
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
    border-left: 1px solid rgb(var(--k-chip-loop-rgb) / 0.4);
    background: transparent;
    color: var(--konjo-flame);
    font-size: 15px;
    cursor: pointer;
  }
  .iterpill .sb:hover {
    background: rgb(var(--k-chip-loop-rgb) / 0.24);
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
  /* ×N color ramp (round 2, item 5) — mirrors StackCard.svelte's identical
     ramp, scoped to the stack pill instead of a card's. */
  .iterpill.tier-yellow {
    border-color: rgb(var(--k-chip-effort-rgb) / 0.5);
    background: rgb(var(--k-chip-effort-rgb) / 0.08);
    color: var(--k-chip-effort);
  }
  .iterpill.tier-yellow .sb {
    border-left-color: rgb(var(--k-chip-effort-rgb) / 0.4);
    color: var(--k-chip-effort);
  }
  .iterpill.tier-yellow .sb:hover {
    background: rgb(var(--k-chip-effort-rgb) / 0.24);
  }
  .iterpill.tier-red {
    border-color: rgb(var(--k-danger-rgb) / 0.5);
    background: rgb(var(--k-danger-rgb) / 0.1);
    color: var(--k-danger);
  }
  .iterpill.tier-red .sb {
    border-left-color: rgb(var(--k-danger-rgb) / 0.4);
    color: var(--k-danger);
  }
  .iterpill.tier-red .sb:hover {
    background: rgb(var(--k-danger-rgb) / 0.24);
  }
  /* Running-loop chrome — mirrors `StackCard.svelte`'s identical pill glow +
     spinner, scoped to the whole stack instead of one card. */
  .iterpill.running {
    animation: iterglow 2.4s ease-in-out infinite;
  }
  @keyframes iterglow {
    0%,
    100% {
      box-shadow: 0 0 0 0 rgb(var(--k-chip-loop-rgb) / 0);
      border-color: rgb(var(--k-chip-loop-rgb) / 0.6);
    }
    50% {
      box-shadow: 0 0 14px 1px rgb(var(--k-chip-loop-rgb) / 0.45);
      border-color: rgb(var(--k-chip-loop-rgb) / 1);
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
  /* UI-3 — banners (cost confirm / stop reason / error / dry run) used to
     live inside `.dockrun`, stacked above the old full-width run button.
     That button now lives in `.dockhead` (see `.hrun` below), so these
     render in their own strip right under the header instead — always
     visible regardless of `dockOpen`, same as before. */
  .dockbanners {
    padding: 0 0 11px;
  }
  .runbanner {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 12px;
    border-radius: 8px;
    background: rgb(var(--k-shadow-rgb) / 0.2);
    border: 1px solid rgb(var(--k-wash-rgb) / 0.16);
    color: rgb(var(--k-text-primary-rgb) / 0.72);
    font-size: 11px;
  }
  .runbanner span {
    flex: 1;
    min-width: 0;
  }
  .runbanner button {
    flex: 0 0 auto;
    background: none;
    border: none;
    color: inherit;
    cursor: pointer;
    display: inline-flex;
  }
  .runbanner button :global(svg) {
    width: 12px;
    height: 12px;
  }
  .runbanner.err {
    background: rgb(var(--k-ext-red-banner-rgb) / 0.1);
    border-color: rgb(var(--k-ext-red-banner-rgb) / 0.4);
    color: rgb(var(--k-ext-red-banner-light-rgb) / 0.95);
  }
  .runbanner.ok {
    background: rgb(var(--k-preset-benchmark-rgb) / 0.1);
    border-color: rgb(var(--k-preset-benchmark-rgb) / 0.4);
    color: rgb(var(--k-ext-green-banner-light-rgb) / 0.95);
  }
  /* Cost-estimate confirm (round 2, item 6) — non-blocking, two explicit
     actions instead of a single dismiss X like the banners above. */
  .costconfirm {
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
    padding: 10px 12px;
    margin-bottom: 9px;
    border-radius: 8px;
    background: rgb(var(--k-chip-effort-rgb) / 0.08);
    border: 1px solid rgb(var(--k-chip-effort-rgb) / 0.35);
    font-size: 11px;
  }
  .costconfirm .ccmsg {
    display: flex;
    align-items: center;
    gap: 6px;
    color: rgb(var(--k-text-primary-rgb) / 0.8);
  }
  .costconfirm .ccmsg :global(svg) {
    width: 13px;
    height: 13px;
    flex: 0 0 auto;
    color: var(--k-chip-effort);
  }
  .costconfirm .ccmsg b {
    color: var(--k-chip-effort);
    font-weight: 700;
  }
  .costconfirm .ccactions {
    display: flex;
    gap: 8px;
  }
  .costconfirm .ccactions button {
    flex: 1;
    padding: 7px 10px;
    border-radius: 6px;
    border: 1px solid rgb(var(--k-chip-effort-rgb) / 0.4);
    background: rgb(var(--k-chip-effort-rgb) / 0.12);
    color: var(--k-chip-effort);
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    font-size: 10.5px;
    font-weight: 700;
    cursor: pointer;
  }
  .costconfirm .ccactions button:hover {
    background: rgb(var(--k-chip-effort-rgb) / 0.22);
  }
  /* UI-3 — the compact run/pause control that now lives in `.dockhead`
     itself, replacing the old full-width `.runsplit` button that sat in its
     own row underneath. `position:relative` so `RunMenu` (an `.runmenu`
     absolutely-positioned child) anchors off this pair, not the whole dock. */
  .hrun {
    position: relative;
    flex: 0 0 auto;
    margin-left: auto;
    display: inline-flex;
    border-radius: 7px;
    overflow: hidden;
    box-shadow: 0 3px 10px rgb(var(--k-chip-loop-rgb) / 0.25);
  }
  .hrunbtn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    background: linear-gradient(180deg, var(--k-ext-flame-grad-a), var(--k-chip-loop));
    color: var(--k-ext-bug-231000);
    border: none;
    padding: 0 14px;
    font-family: var(--font-mono, 'JetBrains Mono', monospace);
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
    white-space: nowrap;
  }
  .hrunbtn :global(svg) {
    width: 13px;
    height: 13px;
  }
  .hrunbtn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .hrunchev {
    background: linear-gradient(180deg, var(--k-ext-flame-grad-c), var(--k-ext-flame-grad-d));
    border: none;
    border-left: 1px solid rgb(var(--k-shadow-rgb) / 0.28);
    color: var(--k-ext-bug-231000);
    padding: 0 9px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
  }
  .hrunchev :global(svg) {
    width: 12px;
    height: 12px;
  }
  .sctrl.drop-before {
    box-shadow: 0 -3px 0 var(--stack-violet, var(--k-chip-model));
  }
  .sctrl.drop-after {
    box-shadow: 0 3px 0 var(--stack-violet, var(--k-chip-model));
  }
  @media (prefers-reduced-motion: reduce) {
    .dockbody {
      transition: none;
    }
    .iterpill.running,
    .iterpill .lb :global(svg.spin) {
      animation: none;
    }
  }
</style>
